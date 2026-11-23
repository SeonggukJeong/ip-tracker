//! 로그 파일 탐색, 미리보기, 적재.

use std::cell::Cell;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, ErrorKind, Read};
use std::net::IpAddr;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use chrono::{DateTime, FixedOffset, NaiveDate};
use flate2::read::MultiGzDecoder;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::db;
use crate::geo::{GeoDb, OriginClass};
use crate::logformat::{detect, LineParser};
use crate::Error;

const PREVIEW_LINES: usize = 20;
const DETECT_LINES: usize = 50;
const SAMPLE_LIMIT: usize = 5;
const PROGRESS_STEP: u64 = 1 << 20;
/// User-Agent 번호 캐시가 이 크기를 넘으면 비운다(메모리 상한).
const UA_CACHE_LIMIT: usize = 100_000;
const BOM: &[u8] = b"\xEF\xBB\xBF";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub path: PathBuf,
    pub server: String,
    /// 끝의 ".gz"를 뗀 파일 이름
    pub filename: String,
    /// 디스크상 크기(바이트, gz는 압축된 크기). 목록 표시용이라 적재 때는 쓰지 않는다.
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRow {
    pub ip: String,
    /// "YYYY-MM-DD HH:MM:SS" KST
    pub time: String,
    pub method: String,
    pub url: String,
    pub status: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub log_format: Option<String>,
    pub rows: Vec<PreviewRow>,
    pub sampled: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictPolicy {
    Ask,
    Replace,
    Skip,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestOptions {
    pub conflict: ConflictPolicy,
    pub allow_high_failure: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSummary {
    pub file_id: i64,
    pub lines: u64,
    pub failed_lines: u64,
    pub foreign_requests: u64,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum IngestOutcome {
    Ingested(FileSummary),
    /// 같은 내용(sha256)이 이미 적재됨
    Duplicate,
    /// 같은 server+filename이 다른 내용으로 적재돼 있고 정책이 Ask
    Conflict,
    /// Conflict인데 정책이 Skip
    Skipped,
    /// 실패율 1% 초과, allow_high_failure=false
    HighFailure { lines: u64, failed_lines: u64, samples: Vec<String> },
}

fn kst() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).unwrap()
}

fn ends_with_gz(name: &str) -> bool {
    name.len() >= 3 && name.as_bytes()[name.len() - 3..].eq_ignore_ascii_case(b".gz")
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// 파일과 폴더(하위 포함)를 받아 일반 파일 목록을 경로순으로 돌려준다.
/// server: server_regex가 있고 파일명에 맞으면 첫 번째 캡처 그룹, 아니면 바로 위 폴더 이름.
pub fn scan(paths: &[PathBuf], server_regex: Option<&regex::Regex>) -> Vec<Candidate> {
    let mut files: Vec<PathBuf> = paths
        .iter()
        .flat_map(|p| {
            walkdir::WalkDir::new(p)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_file())
                .map(walkdir::DirEntry::into_path)
        })
        .collect();
    files.sort();
    files.dedup();
    files
        .into_iter()
        .map(|path| {
            let name = file_name(&path);
            let filename = if ends_with_gz(&name) { name[..name.len() - 3].to_string() } else { name };
            let from_regex = server_regex
                .and_then(|re| re.captures(&filename))
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string());
            let server = from_regex.unwrap_or_else(|| path.parent().map(file_name).unwrap_or_default());
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            Candidate { path, server, filename, size }
        })
        .collect()
}

/// 디스크에서 읽은 바이트 수를 센다(gz는 압축된 크기 기준).
struct CountingReader<R> {
    inner: R,
    count: Rc<Cell<u64>>,
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.count.set(self.count.get() + n as u64);
        Ok(n)
    }
}

/// 줄 끝(\n, \r)과 파일 맨 앞 BOM을 뗀 내용.
fn line_content(raw: &[u8], first: bool) -> &[u8] {
    let mut s = raw;
    if first {
        s = s.strip_prefix(BOM).unwrap_or(s);
    }
    s = s.strip_suffix(b"\n").unwrap_or(s);
    s.strip_suffix(b"\r").unwrap_or(s)
}

fn is_blank(content: &[u8]) -> bool {
    content.iter().all(u8::is_ascii_whitespace)
}

/// 파일을 (gz면 풀어서) 한 줄씩 읽어 f(원래 바이트, 내용)를 부른다.
/// 디스크에서 약 1MB를 더 읽을 때마다 on_progress(지금까지 읽은 디스크 바이트)를 부른다.
/// 압축 해제 오류는 EOF로 보지 않고 Error::CorruptGz로 돌려준다.
fn for_each_line(
    path: &Path,
    mut on_progress: impl FnMut(u64),
    mut f: impl FnMut(&[u8], &[u8]) -> Result<ControlFlow<()>, Error>,
) -> Result<(), Error> {
    let count = Rc::new(Cell::new(0));
    let counted = CountingReader { inner: File::open(path)?, count: Rc::clone(&count) };
    let name = file_name(path);
    let is_gz = ends_with_gz(&name);
    // flate2는 잘린 스트림을 UnexpectedEof로, 깨진 헤더·데이터·CRC를 InvalidInput/InvalidData로 알린다.
    let read_error = |e: io::Error| match e.kind() {
        ErrorKind::UnexpectedEof | ErrorKind::InvalidInput | ErrorKind::InvalidData if is_gz => {
            Error::CorruptGz(name.clone())
        }
        _ => Error::Io(e),
    };
    let mut reader: Box<dyn BufRead> = if is_gz {
        Box::new(BufReader::new(MultiGzDecoder::new(BufReader::new(counted))))
    } else {
        Box::new(BufReader::new(counted))
    };
    let mut buf = Vec::new();
    let mut first = true;
    let mut reported = 0;
    loop {
        buf.clear();
        if reader.read_until(b'\n', &mut buf).map_err(read_error)? == 0 {
            return Ok(());
        }
        let flow = f(&buf, line_content(&buf, first))?;
        first = false;
        if count.get() - reported >= PROGRESS_STEP {
            reported = count.get();
            on_progress(reported);
        }
        if flow.is_break() {
            return Ok(());
        }
    }
}

/// 비어 있지 않은 앞 n줄.
fn head_lines(path: &Path, n: usize) -> Result<Vec<String>, Error> {
    let mut lines = Vec::new();
    if n == 0 {
        return Ok(lines);
    }
    for_each_line(path, |_| {}, |_, content| {
        if !is_blank(content) {
            lines.push(String::from_utf8_lossy(content).into_owned());
        }
        Ok(if lines.len() >= n { ControlFlow::Break(()) } else { ControlFlow::Continue(()) })
    })?;
    Ok(lines)
}

/// 비어 있지 않은 앞 20줄을 읽는다. parser가 None이면 detect로 고르고, 못 고르면 log_format=None, rows는 빈 목록.
/// rows.len() + failed == sampled.
pub fn preview(path: &Path, parser: Option<&LineParser>) -> Result<Preview, Error> {
    let lines = head_lines(path, PREVIEW_LINES)?;
    let detected;
    let parser = match parser {
        Some(p) => p,
        None => match detect_file(path, None)? {
            Some(p) => {
                detected = p;
                &detected
            }
            None => {
                return Ok(Preview { log_format: None, rows: Vec::new(), sampled: lines.len(), failed: lines.len() })
            }
        },
    };
    let rows: Vec<PreviewRow> = lines
        .iter()
        .filter_map(|l| parser.parse(l))
        .map(|r| PreviewRow {
            ip: r.ip.to_canonical().to_string(),
            time: r.ts.with_timezone(&kst()).format("%Y-%m-%d %H:%M:%S").to_string(),
            method: r.method,
            url: r.url,
            status: r.status,
        })
        .collect();
    Ok(Preview {
        log_format: Some(parser.log_format().to_string()),
        failed: lines.len() - rows.len(),
        sampled: lines.len(),
        rows,
    })
}

/// 파일 앞 50줄로 형식을 자동 인식한다.
pub fn detect_file(path: &Path, time_format: Option<&str>) -> Result<Option<LineParser>, Error> {
    Ok(detect(&head_lines(path, DETECT_LINES)?, time_format))
}

pub fn ingest_file(
    conn: &mut Connection,
    geo: &GeoDb,
    parser: &LineParser,
    c: &Candidate,
    opts: &IngestOptions,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<IngestOutcome, Error> {
    let size = std::fs::metadata(&c.path)?.len();
    let total = size * 2;
    let result = ingest_inner(conn, geo, parser, c, opts, size, progress);
    progress(total, total);
    result
}

/// (KST 날짜, IP)별 합계
struct DailyAgg {
    class: OriginClass,
    country: Option<String>,
    requests: u64,
    not_found: u64,
    first_ts: i64,
    last_ts: i64,
}

fn ingest_inner(
    conn: &mut Connection,
    geo: &GeoDb,
    parser: &LineParser,
    c: &Candidate,
    opts: &IngestOptions,
    size: u64,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<IngestOutcome, Error> {
    let total = size * 2;

    // 1차: 해시와 줄 수만. DB는 읽기만 한다.
    let mut hasher = Sha256::new();
    let mut lines = 0u64;
    for_each_line(&c.path, |n| progress(n.min(size), total), |raw, content| {
        hasher.update(raw);
        if !is_blank(content) {
            lines += 1;
        }
        Ok(ControlFlow::Continue(()))
    })?;
    let sha256: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();

    if lines > 0 {
        let dup: Option<i64> =
            conn.query_row("SELECT id FROM ingest_files WHERE sha256=?1 LIMIT 1", [&sha256], |r| r.get(0)).optional()?;
        if dup.is_some() {
            return Ok(IngestOutcome::Duplicate);
        }
    }
    let existing: Option<i64> = conn
        .query_row("SELECT id FROM ingest_files WHERE server=?1 AND filename=?2", [&c.server, &c.filename], |r| {
            r.get(0)
        })
        .optional()?;
    if existing.is_some() {
        match opts.conflict {
            ConflictPolicy::Ask => return Ok(IngestOutcome::Conflict),
            ConflictPolicy::Skip => return Ok(IngestOutcome::Skipped),
            ConflictPolicy::Replace => {}
        }
    }

    // 2차: 트랜잭션 안에서 파싱·적재. 오류로 빠져나가면 tx가 drop되며 롤백된다.
    let tx = conn.transaction()?;
    if let Some(id) = existing {
        tx.execute("DELETE FROM ingest_files WHERE id=?1", [id])?;
    }
    tx.execute(
        "INSERT INTO ingest_files(server, filename, sha256, lines, failed_lines, ingested_at) VALUES(?1, ?2, ?3, 0, 0, ?4)",
        params![c.server, c.filename, sha256, chrono::Utc::now().timestamp()],
    )?;
    let file_id = tx.last_insert_rowid();

    let mut lines = 0u64;
    let mut failed_lines = 0u64;
    let mut foreign_requests = 0u64;
    let mut samples = Vec::new();
    let mut daily: HashMap<(NaiveDate, IpAddr), DailyAgg> = HashMap::new();
    {
        let mut insert_fr = tx.prepare(
            "INSERT INTO foreign_requests(ts, date, ip, country, file_id, method, url, status, bytes, ua_id)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        let mut insert_ua = tx.prepare("INSERT OR IGNORE INTO user_agents(ua) VALUES(?1)")?;
        let mut select_ua = tx.prepare("SELECT id FROM user_agents WHERE ua=?1")?;
        let mut ua_ids: HashMap<String, i64> = HashMap::new();

        for_each_line(&c.path, |n| progress(size + n.min(size), total), |_, content| {
            if is_blank(content) {
                return Ok(ControlFlow::Continue(()));
            }
            lines += 1;
            let text = String::from_utf8_lossy(content);
            let Some(rec) = parser.parse(&text) else {
                failed_lines += 1;
                if samples.len() < SAMPLE_LIMIT {
                    samples.push(text.into_owned());
                }
                return Ok(ControlFlow::Continue(()));
            };
            let ip = rec.ip.to_canonical();
            let local: DateTime<FixedOffset> = rec.ts.with_timezone(&kst());
            let date = local.date_naive();
            let ts = rec.ts.timestamp();
            let agg = daily.entry((date, ip)).or_insert_with(|| {
                let origin = geo.classify(ip);
                DailyAgg { class: origin.class, country: origin.country, requests: 0, not_found: 0, first_ts: ts, last_ts: ts }
            });
            agg.requests += 1;
            agg.not_found += u64::from(rec.status == 404);
            agg.first_ts = agg.first_ts.min(ts);
            agg.last_ts = agg.last_ts.max(ts);

            if agg.class == OriginClass::Foreign {
                let ua_id = match rec.user_agent {
                    None => None,
                    Some(ua) => Some(match ua_ids.get(&ua) {
                        Some(&id) => id,
                        None => {
                            insert_ua.execute([&ua])?;
                            let id: i64 = select_ua.query_row([&ua], |r| r.get(0))?;
                            if ua_ids.len() >= UA_CACHE_LIMIT {
                                ua_ids.clear();
                            }
                            ua_ids.insert(ua, id);
                            id
                        }
                    }),
                };
                insert_fr.execute(params![
                    ts,
                    date.format("%Y-%m-%d").to_string(),
                    ip.to_string(),
                    agg.country,
                    file_id,
                    rec.method,
                    rec.url,
                    rec.status,
                    rec.bytes.map(|b| b as i64),
                    ua_id,
                ])?;
                foreign_requests += 1;
            }
            Ok(ControlFlow::Continue(()))
        })?;
    }

    // failed_lines / lines > 0.01
    if failed_lines * 100 > lines && !opts.allow_high_failure {
        tx.rollback()?;
        return Ok(IngestOutcome::HighFailure { lines, failed_lines, samples });
    }

    {
        let mut insert_daily = tx.prepare(
            "INSERT INTO ip_daily(date, file_id, ip, class, country, requests, not_found, first_ts, last_ts)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        for ((date, ip), a) in &daily {
            insert_daily.execute(params![
                date.format("%Y-%m-%d").to_string(),
                file_id,
                ip.to_string(),
                a.class as u8,
                a.country,
                a.requests as i64,
                a.not_found as i64,
                a.first_ts,
                a.last_ts,
            ])?;
        }
    }
    let date_from = daily.keys().map(|(d, _)| *d).min().map(|d| d.format("%Y-%m-%d").to_string());
    let date_to = daily.keys().map(|(d, _)| *d).max().map(|d| d.format("%Y-%m-%d").to_string());
    tx.execute(
        "UPDATE ingest_files SET date_from=?1, date_to=?2, lines=?3, failed_lines=?4 WHERE id=?5",
        params![date_from, date_to, lines as i64, failed_lines as i64, file_id],
    )?;
    tx.commit()?;
    Ok(IngestOutcome::Ingested(FileSummary { file_id, lines, failed_lines, foreign_requests, date_from, date_to }))
}

/// date(YYYY-MM-DD) 이전의 foreign_requests를 지우고 지운 행 수를 돌려준다. ip_daily는 건드리지 않는다.
/// 지운 뒤 db::compact를 호출한다.
pub fn purge_foreign_before(conn: &Connection, date: &str) -> Result<usize, Error> {
    // 문자열 비교로 지우므로 형식이 틀린 값이 엉뚱한 행을 지우지 않게 먼저 검사한다.
    NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| Error::Other(format!("날짜 형식이 아닙니다: {date}")))?;
    let n = conn.execute("DELETE FROM foreign_requests WHERE date < ?1", [date])?;
    db::compact(conn)?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::logformat::COMBINED;
    use flate2::write::GzEncoder;
    use rusqlite::Connection;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    const GEO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../src-tauri/resources/dbip-country-lite.mmdb");
    const KR: &str = "168.126.63.1";
    const US: &str = "8.8.8.8";
    const INTERNAL: &str = "10.0.0.1";
    const T: &str = "22/Nov/2026:23:59:59 +0900";

    fn write_bytes(dir: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        p
    }
    fn joined(lines: &[String]) -> Vec<u8> {
        lines.iter().map(|l| format!("{l}\n")).collect::<String>().into_bytes()
    }
    fn gz(bytes: &[u8]) -> Vec<u8> {
        let mut e = GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(bytes).unwrap();
        e.finish().unwrap()
    }
    fn write(dir: &Path, rel: &str, lines: &[String]) -> PathBuf {
        write_bytes(dir, rel, &joined(lines))
    }
    fn write_gz(dir: &Path, rel: &str, lines: &[String]) -> PathBuf {
        write_bytes(dir, rel, &gz(&joined(lines)))
    }
    fn line(ip: &str, time: &str, req: &str, status: u16) -> String {
        format!(r#"{ip} - - [{time}] "{req} HTTP/1.1" {status} 512 "-" "Mozilla/5.0 (Test)""#)
    }
    fn setup() -> (Connection, GeoDb, LineParser) {
        (db::open_in_memory().unwrap(), GeoDb::open(Path::new(GEO)).unwrap(), LineParser::new(COMBINED, None).unwrap())
    }
    fn cand(p: &Path) -> Candidate {
        scan(&[p.to_path_buf()], None).remove(0)
    }
    fn ingest_result(
        conn: &mut Connection, geo: &GeoDb, parser: &LineParser, p: &Path, conflict: ConflictPolicy, allow: bool,
    ) -> Result<IngestOutcome, crate::Error> {
        let opts = IngestOptions { conflict, allow_high_failure: allow };
        ingest_file(conn, geo, parser, &cand(p), &opts, &mut |_, _| {})
    }
    fn ingest(conn: &mut Connection, geo: &GeoDb, parser: &LineParser, p: &Path) -> IngestOutcome {
        ingest_result(conn, geo, parser, p, ConflictPolicy::Ask, false).unwrap()
    }
    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }
    fn sum_requests(conn: &Connection) -> i64 {
        conn.query_row("SELECT coalesce(sum(requests),0) FROM ip_daily", [], |r| r.get(0)).unwrap()
    }
    fn dates_in_ip_daily(conn: &Connection) -> Vec<String> {
        let mut st = conn.prepare("SELECT DISTINCT date FROM ip_daily ORDER BY date").unwrap();
        st.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    }
    fn us_lines(n: usize) -> Vec<String> {
        (0..n).map(|i| line(US, T, &format!("GET /p{i}"), 200)).collect()
    }

    #[test]
    fn scan_uses_parent_folder_as_server_and_strips_gz() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "was01/access.2026-11-22", &us_lines(1));
        write_gz(dir.path(), "was02/access.2026-11-22.gz", &us_lines(1));
        let cs = scan(&[dir.path().to_path_buf()], None);
        assert_eq!(
            cs.iter().map(|c| (c.server.as_str(), c.filename.as_str())).collect::<Vec<_>>(),
            vec![("was01", "access.2026-11-22"), ("was02", "access.2026-11-22")]
        );
        let on_disk = std::fs::metadata(dir.path().join("was02/access.2026-11-22.gz")).unwrap().len();
        assert_eq!(cs[1].size, on_disk);
        assert!(cs[0].size > 0);
    }

    #[test]
    fn scan_server_regex_wins_when_it_matches() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "logs/was07_access.log", &us_lines(1));
        write(dir.path(), "logs/other.log", &us_lines(1));
        let re = regex::Regex::new(r"^(was\d+)_").unwrap();
        let cs = scan(&[dir.path().to_path_buf()], Some(&re));
        // 경로순: other.log, was07_access.log
        assert_eq!(cs.iter().map(|c| c.server.as_str()).collect::<Vec<_>>(), vec!["logs", "was07"]);
    }

    #[test]
    fn preview_shows_first_20_rows_in_kst() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "was01/access.log", &us_lines(30));
        let p = preview(&path, None).unwrap();
        assert_eq!((p.rows.len(), p.sampled, p.failed), (20, 20, 0));
        assert_eq!(p.log_format.as_deref(), Some(COMBINED));
        assert_eq!(p.rows[0].time, "2026-11-22 23:59:59");
    }

    #[test]
    fn preview_of_unrecognized_file_has_no_format() {
        let dir = tempfile::tempdir().unwrap();
        let garbage: Vec<String> = (0..10).map(|i| format!("이건 로그가 아님 {i}")).collect();
        let garbage_path = write(dir.path(), "was01/garbage.txt", &garbage);
        let p = preview(&garbage_path, None).unwrap();
        assert!(p.log_format.is_none() && p.rows.is_empty());
    }

    #[test]
    fn ingest_aggregates_all_ips_and_keeps_foreign_rows_only() {
        let (mut conn, geo, parser) = setup();
        let dir = tempfile::tempdir().unwrap();
        let f = write(
            dir.path(),
            "was01/access.log",
            &[
                line(KR, T, "GET /a", 200),
                line(KR, T, "GET /b", 404),
                line(KR, T, "GET /c", 200),
                line(US, T, "GET /a", 200),
                line(US, T, "GET /x", 404),
                line(INTERNAL, T, "GET /a", 200),
            ],
        );
        let IngestOutcome::Ingested(s) = ingest(&mut conn, &geo, &parser, &f) else { panic!() };
        assert_eq!((s.lines, s.failed_lines, s.foreign_requests), (6, 0, 2));
        assert_eq!((s.date_from.as_deref(), s.date_to.as_deref()), (Some("2026-11-22"), Some("2026-11-22")));

        let mut st = conn
            .prepare("SELECT ip, class, country, requests, not_found FROM ip_daily ORDER BY class")
            .unwrap();
        let rows: Vec<(String, i64, Option<String>, i64, i64)> = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(
            rows,
            vec![
                (KR.to_string(), 0, Some("KR".to_string()), 3, 1),
                (US.to_string(), 1, Some("US".to_string()), 2, 1),
                (INTERNAL.to_string(), 2, None, 1, 0),
            ]
        );

        let mut st = conn.prepare("SELECT ip, country, file_id FROM foreign_requests").unwrap();
        let fr: Vec<(String, String, i64)> =
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect();
        assert_eq!(fr, vec![(US.to_string(), "US".to_string(), s.file_id); 2]);

        assert_eq!(count(&conn, "user_agents"), 1);
    }

    #[test]
    fn date_boundary_follows_kst() {
        let (mut conn, geo, parser) = setup();
        let dir = tempfile::tempdir().unwrap();
        let f = write(dir.path(), "was01/access.log", &[line(US, "22/Nov/2026:15:30:00 +0000", "GET /a", 200)]);
        assert!(matches!(ingest(&mut conn, &geo, &parser, &f), IngestOutcome::Ingested(_)));
        assert_eq!(dates_in_ip_daily(&conn), vec!["2026-11-23"]);
    }

    #[test]
    fn gz_and_plain_of_same_content_are_duplicates() {
        let (mut conn, geo, parser) = setup();
        let dir = tempfile::tempdir().unwrap();
        let plain = write(dir.path(), "was01/access.log", &us_lines(3));
        let gz_same_content_other_server = write_gz(dir.path(), "was02/access.log.gz", &us_lines(3));
        assert!(matches!(ingest(&mut conn, &geo, &parser, &plain), IngestOutcome::Ingested(_)));
        assert!(matches!(ingest(&mut conn, &geo, &parser, &gz_same_content_other_server), IngestOutcome::Duplicate));
        assert_eq!(count(&conn, "ingest_files"), 1);
    }

    #[test]
    fn same_name_different_content_asks_then_replaces_exactly() {
        let (mut conn, geo, parser) = setup();
        let dir = tempfile::tempdir().unwrap();
        let v1 = write(dir.path(), "was01/access.log", &us_lines(2));
        assert!(matches!(ingest(&mut conn, &geo, &parser, &v1), IngestOutcome::Ingested(_)));
        let v2 = write(dir.path(), "was01/access.log", &us_lines(5));
        let mut with = |p: ConflictPolicy| ingest_result(&mut conn, &geo, &parser, &v2, p, false).unwrap();
        assert!(matches!(with(ConflictPolicy::Ask), IngestOutcome::Conflict));
        assert!(matches!(with(ConflictPolicy::Skip), IngestOutcome::Skipped));
        assert_eq!(sum_requests(&conn), 2);
        assert!(matches!(
            ingest_result(&mut conn, &geo, &parser, &v2, ConflictPolicy::Replace, false).unwrap(),
            IngestOutcome::Ingested(_)
        ));
        assert_eq!((sum_requests(&conn), count(&conn, "foreign_requests"), count(&conn, "ingest_files")), (5, 5, 1));
    }

    #[test]
    fn split_files_of_same_day_both_ingest() {
        let (mut conn, geo, parser) = setup();
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "was01/access.2026-11-22", &us_lines(2));
        let b = write(dir.path(), "was01/access.2026-11-22_1", &[line(KR, T, "GET /z", 200)]);
        assert!(matches!(ingest(&mut conn, &geo, &parser, &a), IngestOutcome::Ingested(_)));
        assert!(matches!(ingest(&mut conn, &geo, &parser, &b), IngestOutcome::Ingested(_)));
        assert_eq!(count(&conn, "ingest_files"), 2);
    }

    #[test]
    fn high_failure_rolls_back_until_allowed() {
        let (mut conn, geo, parser) = setup();
        let dir = tempfile::tempdir().unwrap();
        let mut lines = us_lines(90);
        lines.extend((0..10).map(|i| format!("broken line {i}")));
        let f = write(dir.path(), "was01/access.log", &lines);
        let IngestOutcome::HighFailure { lines, failed_lines, samples } = ingest(&mut conn, &geo, &parser, &f) else {
            panic!()
        };
        assert_eq!((lines, failed_lines, samples.len()), (100, 10, 5));
        assert_eq!(samples[0], "broken line 0");
        assert_eq!((count(&conn, "ingest_files"), count(&conn, "foreign_requests")), (0, 0));
        assert!(matches!(
            ingest_result(&mut conn, &geo, &parser, &f, ConflictPolicy::Ask, true).unwrap(),
            IngestOutcome::Ingested(_)
        ));
    }

    #[test]
    fn failure_at_or_below_one_percent_is_accepted() {
        let (mut conn, geo, parser) = setup();
        let dir = tempfile::tempdir().unwrap();
        let mut lines = us_lines(99);
        lines.push("broken".to_string());
        let f = write(dir.path(), "was01/access.log", &lines);
        let IngestOutcome::Ingested(s) = ingest(&mut conn, &geo, &parser, &f) else { panic!() };
        assert_eq!((s.lines, s.failed_lines), (100, 1));
    }

    #[test]
    fn non_utf8_bytes_do_not_abort_the_file() {
        let (mut conn, geo, parser) = setup();
        let dir = tempfile::tempdir().unwrap();
        let mut bytes = format!("{}\n", line(US, T, "GET /a", 200)).into_bytes();
        bytes.extend(format!(r#"{US} - - [{T}] "GET /b"#).as_bytes());
        bytes.extend([0xFF, 0xFE]);
        bytes.extend(br#" HTTP/1.1" 200 512 "-" "UA""#);
        bytes.push(b'\n');
        bytes.extend(format!("{}\n", line(US, T, "GET /c", 200)).as_bytes());
        let f = write_bytes(dir.path(), "was01/access.log", &bytes);
        let IngestOutcome::Ingested(s) = ingest(&mut conn, &geo, &parser, &f) else { panic!() };
        assert_eq!((s.lines, s.failed_lines), (3, 0));
    }

    #[test]
    fn truncated_gz_is_an_error_and_leaves_nothing() {
        let (mut conn, geo, parser) = setup();
        let dir = tempfile::tempdir().unwrap();
        let full = gz(&joined(&us_lines(500)));
        let f = write_bytes(dir.path(), "was01/access.log.gz", &full[..full.len() / 2]);
        let e = ingest_result(&mut conn, &geo, &parser, &f, ConflictPolicy::Ask, false).unwrap_err();
        assert_eq!(e.to_string(), "압축 파일이 잘렸거나 손상됐습니다: access.log.gz");
        assert_eq!((count(&conn, "ingest_files"), count(&conn, "ip_daily")), (0, 0));
        // 헤더부터 깨진 .gz도 같은 문구로 알린다.
        let garbage = write_bytes(dir.path(), "was01/garbage.gz", b"not gzip at all\n");
        let e = ingest_result(&mut conn, &geo, &parser, &garbage, ConflictPolicy::Ask, false).unwrap_err();
        assert_eq!(e.to_string(), "압축 파일이 잘렸거나 손상됐습니다: garbage.gz");
    }

    #[test]
    fn empty_file_ingests_as_zero_lines() {
        let (mut conn, geo, parser) = setup();
        let dir = tempfile::tempdir().unwrap();
        let empty = write_bytes(dir.path(), "was01/access.log", b"");
        let IngestOutcome::Ingested(s) = ingest(&mut conn, &geo, &parser, &empty) else { panic!() };
        assert_eq!((s.lines, s.date_from), (0, None));
    }

    #[test]
    fn two_empty_files_are_not_duplicates() {
        let (mut conn, geo, parser) = setup();
        let dir = tempfile::tempdir().unwrap();
        let a = write_bytes(dir.path(), "was01/a.log", b"");
        let b = write_bytes(dir.path(), "was02/b.log", b"");
        assert!(matches!(ingest(&mut conn, &geo, &parser, &a), IngestOutcome::Ingested(_)));
        assert!(matches!(ingest(&mut conn, &geo, &parser, &b), IngestOutcome::Ingested(_)));
        assert_eq!(count(&conn, "ingest_files"), 2);
    }

    #[test]
    fn crlf_file_parses_like_lf() {
        let (mut conn, geo, parser) = setup();
        let dir = tempfile::tempdir().unwrap();
        let text: String = us_lines(3).iter().map(|l| format!("{l}\r\n")).collect();
        let f = write_bytes(dir.path(), "was01/access.log", text.as_bytes());
        let IngestOutcome::Ingested(s) = ingest(&mut conn, &geo, &parser, &f) else { panic!() };
        assert_eq!((s.lines, s.failed_lines, s.foreign_requests), (3, 0, 3));
        let ua: String = conn
            .query_row(
                "SELECT ua FROM user_agents JOIN foreign_requests f ON f.ua_id = user_agents.id LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(ua, "Mozilla/5.0 (Test)");
    }

    #[test]
    fn duplicate_is_decided_before_any_db_write() {
        let (mut conn, geo, parser) = setup();
        let dir = tempfile::tempdir().unwrap();
        let f = write(dir.path(), "was01/access.log", &us_lines(3));
        assert!(matches!(ingest(&mut conn, &geo, &parser, &f), IngestOutcome::Ingested(_)));
        let counts = |c: &Connection| (count(c, "ingest_files"), count(c, "ip_daily"), count(c, "foreign_requests"));
        let before = counts(&conn);
        let changes_before = conn.total_changes();
        assert!(matches!(ingest(&mut conn, &geo, &parser, &f), IngestOutcome::Duplicate));
        assert_eq!(counts(&conn), before);
        assert_eq!(conn.total_changes(), changes_before);
    }

    #[test]
    fn serde_shape_is_camel_case() {
        let v = serde_json::to_value(IngestOutcome::HighFailure { lines: 100, failed_lines: 10, samples: vec![] }).unwrap();
        assert_eq!(v, serde_json::json!({"kind": "highFailure", "lines": 100, "failedLines": 10, "samples": []}));
        let v = serde_json::to_value(IngestOutcome::Ingested(FileSummary {
            file_id: 1,
            lines: 2,
            failed_lines: 0,
            foreign_requests: 1,
            date_from: None,
            date_to: None,
        }))
        .unwrap();
        assert_eq!(v["kind"], "ingested");
        assert_eq!(v["fileId"], 1);
        assert_eq!(v["foreignRequests"], 1);
    }

    #[test]
    fn mapped_ipv6_is_stored_as_ipv4() {
        let (mut conn, geo, parser) = setup();
        let dir = tempfile::tempdir().unwrap();
        let f = write(dir.path(), "was01/access.log", &[line("::ffff:8.8.8.8", T, "GET /a", 200)]);
        assert!(matches!(ingest(&mut conn, &geo, &parser, &f), IngestOutcome::Ingested(_)));
        let ip: String = conn.query_row("SELECT ip FROM ip_daily", [], |r| r.get(0)).unwrap();
        assert_eq!(ip, "8.8.8.8");
        let ip: String = conn.query_row("SELECT ip FROM foreign_requests", [], |r| r.get(0)).unwrap();
        assert_eq!(ip, "8.8.8.8");
    }

    #[test]
    fn purge_removes_old_foreign_rows_but_keeps_summary() {
        let (mut conn, geo, parser) = setup();
        let dir = tempfile::tempdir().unwrap();
        let f = write(
            dir.path(),
            "was01/access.log",
            &[line(US, "20/Nov/2026:12:00:00 +0900", "GET /a", 200), line(US, "22/Nov/2026:12:00:00 +0900", "GET /a", 200)],
        );
        assert!(matches!(ingest(&mut conn, &geo, &parser, &f), IngestOutcome::Ingested(_)));
        assert_eq!(purge_foreign_before(&conn, "2026-11-21").unwrap(), 1);
        assert_eq!((count(&conn, "foreign_requests"), count(&conn, "ip_daily")), (1, 2));
    }

    #[test]
    fn progress_reaches_total() {
        let (mut conn, geo, parser) = setup();
        let dir = tempfile::tempdir().unwrap();
        let f = write(dir.path(), "was01/access.log", &us_lines(10));
        let total = std::fs::metadata(&f).unwrap().len() * 2;
        let opts = IngestOptions { conflict: ConflictPolicy::Ask, allow_high_failure: false };
        for expected_dup in [false, true] {
            let mut calls = Vec::new();
            let out = ingest_file(&mut conn, &geo, &parser, &cand(&f), &opts, &mut |a, b| calls.push((a, b))).unwrap();
            assert_eq!(matches!(out, IngestOutcome::Duplicate), expected_dup);
            assert_eq!(calls.last(), Some(&(total, total)));
            assert!(calls.iter().all(|&(a, b)| b == total && a <= total));
        }
    }
}
