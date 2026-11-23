use chrono::{DateTime, FixedOffset, NaiveDateTime};
use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq)]
pub struct AccessRecord {
    pub ip: IpAddr,
    pub ts: DateTime<FixedOffset>,
    pub method: String,
    pub url: String,
    pub status: u16,
    pub bytes: Option<u64>,
    pub user_agent: Option<String>,
}

pub const COMMON: &str = r#"%h %l %u %t "%r" %>s %b"#;
pub const COMBINED: &str =
    r#"%h %l %u %t "%r" %>s %b "%{Referer}i" "%{User-Agent}i""#;

const TIME_FORMATS: [&str; 6] = [
    "%d/%b/%Y:%H:%M:%S %z",
    "%d/%b/%Y:%H:%M:%S",
    "%Y-%m-%d %H:%M:%S%.f",
    "%Y-%m-%d %H:%M:%S,%3f",
    "%Y-%m-%dT%H:%M:%S%z",
    "%Y-%m-%dT%H:%M:%S",
];

/// 시간대가 없는 시각에 붙이는 기본 시간대(KST).
const DEFAULT_OFFSET_SECS: i32 = 9 * 3600;

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("로그 형식에 클라이언트 IP 지시자(%h, %a)가 없습니다")]
    MissingIp,
    #[error("로그 형식에 시간 지시자(%t, %{{형식}}t)가 없습니다")]
    MissingTime,
    #[error("로그 형식에 상태 코드 지시자(%s, %>s)가 없습니다")]
    MissingStatus,
}

#[derive(Debug, Clone)]
enum Tok {
    Lit(char),
    Ip,
    /// `bracketed`는 `%t`(대괄호 포함 출력), 아니면 `%{형식}t`.
    Time { bracketed: bool, fmt: Option<String> },
    Request,
    Status,
    Bytes,
    UserAgent,
    Skip,
}

#[derive(Debug, Clone)]
struct Item {
    tok: Tok,
    /// 큰따옴표 리터럴 안에 있는 필드인지(`\"` 이스케이프 처리 대상).
    quoted: bool,
}

#[derive(Debug, Clone)]
pub struct LineParser {
    log_format: String,
    items: Vec<Item>,
}

impl LineParser {
    /// time_format은 chrono strftime 문자열. None이면 TIME_FORMATS를 차례로 시도한다.
    pub fn new(log_format: &str, time_format: Option<&str>) -> Result<LineParser, FormatError> {
        let items = compile(log_format, time_format);
        let has = |f: fn(&Tok) -> bool| items.iter().any(|i| f(&i.tok));
        if !has(|t| matches!(t, Tok::Ip)) {
            return Err(FormatError::MissingIp);
        }
        if !has(|t| matches!(t, Tok::Time { .. })) {
            return Err(FormatError::MissingTime);
        }
        if !has(|t| matches!(t, Tok::Status)) {
            return Err(FormatError::MissingStatus);
        }
        Ok(LineParser { log_format: log_format.to_string(), items })
    }

    pub fn log_format(&self) -> &str {
        &self.log_format
    }

    pub fn parse(&self, line: &str) -> Option<AccessRecord> {
        let mut rest = line.strip_suffix('\r').unwrap_or(line);
        let mut ip = None;
        let mut ts = None;
        let mut request = None;
        let mut status = None;
        let mut bytes: Option<Option<u64>> = None;
        let mut user_agent: Option<Option<String>> = None;

        for (idx, item) in self.items.iter().enumerate() {
            // 형식상 바로 다음 리터럴 문자(필드가 끝나는 지점).
            let term = match self.items.get(idx + 1).map(|i| &i.tok) {
                Some(Tok::Lit(c)) => Some(*c),
                _ => None,
            };
            let next_is_field = matches!(self.items.get(idx + 1), Some(i) if !matches!(i.tok, Tok::Lit(_)));
            let value = match &item.tok {
                Tok::Lit(c) => {
                    rest = rest.strip_prefix(*c)?;
                    continue;
                }
                Tok::Time { bracketed: true, .. } => {
                    let inner = rest.strip_prefix('[')?;
                    let end = inner.find(']')?;
                    rest = &inner[end + 1..];
                    &inner[..end]
                }
                Tok::Time { fmt, .. } => {
                    let spaces = fmt.as_deref().map_or(0, |f| f.matches(' ').count());
                    let (v, r) = take_time_chunks(rest, spaces, term);
                    rest = r;
                    v
                }
                _ => {
                    let (v, r) = read_field(rest, item.quoted, term, next_is_field)?;
                    rest = r;
                    v
                }
            };
            match &item.tok {
                Tok::Ip if ip.is_none() => ip = Some(value.parse::<IpAddr>().ok()?),
                Tok::Time { fmt, .. } if ts.is_none() => ts = Some(parse_time(value, fmt.as_deref())?),
                Tok::Request if request.is_none() => request = Some(split_request(value)),
                Tok::Status if status.is_none() => status = Some(value.parse::<u16>().ok()?),
                Tok::Bytes if bytes.is_none() => {
                    bytes = Some(if value == "-" { None } else { Some(value.parse::<u64>().ok()?) });
                }
                Tok::UserAgent if user_agent.is_none() => {
                    user_agent = Some(if value == "-" { None } else { Some(value.to_string()) });
                }
                _ => {}
            }
        }
        if !rest.is_empty() {
            return None;
        }
        let (method, url) = request.unwrap_or(("", ""));
        Some(AccessRecord {
            ip: ip?,
            ts: ts?,
            method: method.to_string(),
            url: url.to_string(),
            status: status?,
            bytes: bytes.flatten(),
            user_agent: user_agent.flatten(),
        })
    }
}

/// 비어 있지 않은 표본 줄로 형식을 고른다. 후보 중 성공률이 가장 높고 0.9 이상인 것.
/// time_format이 Some이면 모든 후보 LogFormat에 그 시간 형식을 쓴다.
pub fn detect(sample: &[String], time_format: Option<&str>) -> Option<LineParser> {
    let lines: Vec<&str> = sample.iter().map(String::as_str).filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return None;
    }
    let candidates = [
        COMBINED.to_string(),
        COMMON.to_string(),
        format!("{COMBINED} %D"),
        format!("%v {COMBINED}"),
    ];
    let mut best: Option<(f64, LineParser)> = None;
    for fmt in &candidates {
        let Ok(parser) = LineParser::new(fmt, time_format) else { continue };
        let ok = lines.iter().filter(|l| parser.parse(l).is_some()).count();
        let rate = ok as f64 / lines.len() as f64;
        if rate >= 0.9 && best.as_ref().map_or(true, |(b, _)| rate > *b) {
            best = Some((rate, parser));
        }
    }
    best.map(|(_, p)| p)
}

fn compile(fmt: &str, time_format: Option<&str>) -> Vec<Item> {
    let chars: Vec<char> = fmt.chars().collect();
    let mut items = Vec::new();
    let mut in_quote = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        if c != '%' {
            if c == '"' {
                in_quote = !in_quote;
            }
            items.push(Item { tok: Tok::Lit(c), quoted: false });
            continue;
        }
        if i >= chars.len() {
            items.push(Item { tok: Tok::Lit('%'), quoted: false });
            break;
        }
        if chars[i] == '%' {
            i += 1;
            items.push(Item { tok: Tok::Lit('%'), quoted: false });
            continue;
        }
        // 수식자(%>s, %<s, %!200h, %400,501h 등)는 건너뛴다.
        while i < chars.len() && matches!(chars[i], '<' | '>' | '!' | ',' | '0'..='9') {
            i += 1;
        }
        let mut arg = None;
        if i < chars.len() && chars[i] == '{' {
            if let Some(close) = chars[i..].iter().position(|&c| c == '}') {
                arg = Some(chars[i + 1..i + close].iter().collect::<String>());
                i += close + 1;
            }
        }
        let Some(&d) = chars.get(i) else { break };
        i += 1;
        let tok = match d {
            'h' | 'a' => Tok::Ip,
            't' => match arg {
                Some(f) => Tok::Time { bracketed: false, fmt: Some(f) },
                None => Tok::Time { bracketed: true, fmt: time_format.map(str::to_string) },
            },
            'r' => Tok::Request,
            's' => Tok::Status,
            'b' | 'B' | 'O' => Tok::Bytes,
            'i' if arg.as_deref().is_some_and(|a| a.eq_ignore_ascii_case("User-Agent")) => Tok::UserAgent,
            _ => Tok::Skip,
        };
        items.push(Item { tok, quoted: in_quote });
    }
    items
}

/// 일반 필드 하나를 읽고 (값, 나머지)를 돌려준다.
fn read_field(rest: &str, quoted: bool, term: Option<char>, next_is_field: bool) -> Option<(&str, &str)> {
    let end = if quoted {
        let mut it = rest.char_indices();
        loop {
            match it.next() {
                Some((_, '\\')) => {
                    it.next();
                }
                Some((i, '"')) => break i,
                Some(_) => {}
                None => return None,
            }
        }
    } else if let Some(c) = term {
        rest.find(c)?
    } else if next_is_field {
        rest.find(char::is_whitespace).unwrap_or(rest.len())
    } else {
        rest.len()
    };
    Some((&rest[..end], &rest[end..]))
}

/// `%{형식}t` 값: 형식 안의 공백 수 + 1개의 공백 구분 덩어리. 마지막 덩어리는
/// 형식상 다음 리터럴(예: `]`)이 나오면 거기서 끝난다.
fn take_time_chunks(rest: &str, spaces: usize, term: Option<char>) -> (&str, &str) {
    let mut seen = 0;
    for (i, c) in rest.char_indices() {
        if c == ' ' {
            if seen == spaces {
                return rest.split_at(i);
            }
            seen += 1;
        } else if seen == spaces && term == Some(c) {
            return rest.split_at(i);
        }
    }
    (rest, "")
}

fn parse_time(value: &str, fmt: Option<&str>) -> Option<DateTime<FixedOffset>> {
    match fmt {
        Some(f) => parse_time_with(value, f),
        None => TIME_FORMATS.iter().find_map(|f| parse_time_with(value, f)),
    }
}

fn parse_time_with(value: &str, fmt: &str) -> Option<DateTime<FixedOffset>> {
    if let Ok(dt) = DateTime::parse_from_str(value, fmt) {
        return Some(dt);
    }
    let naive = NaiveDateTime::parse_from_str(value, fmt).ok()?;
    naive.and_local_timezone(FixedOffset::east_opt(DEFAULT_OFFSET_SECS)?).single()
}

/// 요청 줄이 `메서드 URL 프로토콜` 3조각일 때만 메서드와 URL을 돌려준다.
fn split_request(value: &str) -> (&str, &str) {
    let mut p = value.split(' ');
    match (p.next(), p.next(), p.next(), p.next()) {
        (Some(m), Some(u), Some(proto), None) if !m.is_empty() && !u.is_empty() && !proto.is_empty() => (m, u),
        _ => ("", ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const L_COMBINED: &str = r#"203.0.113.7 - - [22/Nov/2026:23:58:01 +0900] "GET /api/v1/items?id=3 HTTP/1.1" 200 5123 "-" "okhttp/4.9.3""#;

    #[test] fn parses_combined() {
        let r = LineParser::new(COMBINED, None).unwrap().parse(L_COMBINED).unwrap();
        assert_eq!(r.ip.to_string(), "203.0.113.7");
        assert_eq!(r.ts.to_rfc3339(), "2026-11-22T23:58:01+09:00");
        assert_eq!((r.method.as_str(), r.url.as_str(), r.status), ("GET", "/api/v1/items?id=3", 200));
        assert_eq!(r.bytes, Some(5123));
        assert_eq!(r.user_agent.as_deref(), Some("okhttp/4.9.3"));
    }
    #[test] fn parses_common_with_dash_bytes() {
        let r = LineParser::new(COMMON, None).unwrap()
            .parse(r#"10.0.0.5 - - [22/Nov/2026:00:00:00 +0900] "POST /login HTTP/1.1" 404 -"#).unwrap();
        assert_eq!((r.status, r.bytes, r.user_agent), (404, None, None));
    }
    #[test] fn custom_time_directive_with_and_without_literal_brackets() {
        let p = LineParser::new(r#"%h [%{%Y-%m-%d %H:%M:%S}t] "%r" %>s"#, None).unwrap();
        let r = p.parse(r#"1.2.3.4 [2026-11-23 14:00:03] "GET / HTTP/1.1" 200"#).unwrap();
        assert_eq!(r.ts.to_rfc3339(), "2026-11-23T14:00:03+09:00"); // 시간대 없음 → KST
        let q = LineParser::new(r#"%h %{%Y-%m-%d %H:%M:%S}t "%r" %>s"#, None).unwrap();
        let r = q.parse(r#"1.2.3.4 2026-11-23 14:00:03 "GET / HTTP/1.1" 200"#).unwrap();
        assert_eq!((r.ts.to_rfc3339().as_str(), r.status), ("2026-11-23T14:00:03+09:00", 200));
    }
    #[test] fn given_time_format_applies_to_percent_t() {
        let line = r#"1.2.3.4 - - [23.11.2026 14:00:03] "GET / HTTP/1.1" 200 1"#;
        let p = LineParser::new(COMMON, Some("%d.%m.%Y %H:%M:%S")).unwrap();
        assert_eq!(p.parse(line).unwrap().ts.to_rfc3339(), "2026-11-23T14:00:03+09:00");
        let s = vec![line.to_string(); 10];
        assert!(detect(&s, None).is_none());
        assert_eq!(detect(&s, Some("%d.%m.%Y %H:%M:%S")).unwrap().log_format(), COMMON);
    }
    #[test] fn trailing_cr_is_ignored() {
        let r = LineParser::new(COMBINED, None).unwrap().parse(&format!("{L_COMBINED}\r")).unwrap();
        assert_eq!(r.user_agent.as_deref(), Some("okhttp/4.9.3"));
    }
    #[test] fn ipv6_and_mapped_addresses_parse() {
        let p = LineParser::new(COMMON, None).unwrap();
        for ip in ["2001:db8::1", "::ffff:203.0.113.7"] {
            let line = format!(r#"{ip} - - [22/Nov/2026:00:00:00 +0900] "GET / HTTP/1.1" 200 1"#);
            assert_eq!(p.parse(&line).unwrap().ip.to_string(), ip.parse::<std::net::IpAddr>().unwrap().to_string());
        }
    }
    #[test] fn broken_request_field_is_kept_with_empty_url() {
        let p = LineParser::new(COMBINED, None).unwrap();
        for req in ["-", r"\x16\x03\x01\x02", ""] {
            let line = format!(r#"198.51.100.9 - - [22/Nov/2026:00:00:00 +0900] "{req}" 400 226 "-" "-""#);
            let r = p.parse(&line).unwrap();
            assert_eq!((r.method.as_str(), r.url.as_str(), r.status), ("", "", 400));
        }
    }
    #[test] fn escaped_quote_inside_request() {
        let line = r#"198.51.100.9 - - [22/Nov/2026:00:00:00 +0900] "GET /a?q=\"x\" HTTP/1.1" 404 10 "-" "curl/8""#;
        let r = LineParser::new(COMBINED, None).unwrap().parse(line).unwrap();
        assert_eq!((r.url.as_str(), r.status), (r#"/a?q=\"x\""#, 404));
    }
    #[test] fn garbage_line_is_none() {
        assert!(LineParser::new(COMBINED, None).unwrap().parse("not a log line").is_none());
    }
    #[test] fn format_without_ip_is_rejected() {
        assert!(matches!(LineParser::new(r#"%t "%r" %>s"#, None), Err(FormatError::MissingIp)));
    }
    #[test] fn detect_picks_combined_then_common() {
        let s = vec![L_COMBINED.to_string(); 10];
        assert_eq!(detect(&s, None).unwrap().log_format(), COMBINED);
        let c = vec![r#"10.0.0.5 - - [22/Nov/2026:00:00:00 +0900] "GET / HTTP/1.1" 200 1"#.to_string(); 10];
        assert_eq!(detect(&c, None).unwrap().log_format(), COMMON);
        assert!(detect(&vec!["zzz".to_string(); 10], None).is_none());
    }
}
