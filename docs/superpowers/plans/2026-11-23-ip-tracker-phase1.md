# 접속 국가 집계 대시보드 1단계 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apache access 로그를 적재해 국가별 요청 수·고유 IP 수를 보여주고 A4 보고서를 내보내는, 인터넷 없이 동작하는 Windows용 Tauri 앱을 만든다.

**Architecture:** Tauri 의존이 없는 Rust 라이브러리 `iptracker-core`(코드에서는 `iptracker_core`)가 로그 파싱, 국가 판별, SQLite 적재, 집계 질의를 모두 담당한다. `src-tauri`는 이 라이브러리를 Tauri 명령으로 감싸는 얇은 층이고, 화면은 React + ECharts로 명령만 호출한다. 가짜 로그 생성기 `loggen`이 실제 로그를 대신해 개발·검증에 쓰인다.

**Tech Stack:** Rust 1.97, Tauri 2, rusqlite(bundled), maxminddb, flate2, chrono, regex, sha2 / Vite, React 18, TypeScript, ECharts 5, Vitest / GitHub Actions(windows-latest, NSIS)

**Spec:** `docs/superpowers/specs/2026-11-23-ip-tracker-design.md`
**목업:** `docs/superpowers/specs/mockups/01-dashboard-full.html`, `docs/superpowers/specs/mockups/02-404-and-report-final.html`. 화면 작업(Task 8~10)은 구현 전에 이 두 파일을 반드시 읽고 항목·배치를 따른다.

## Global Constraints

- 실행 중 인터넷 접근이 전혀 없어야 한다. CDN, 온라인 지도 타일, 온라인 GeoIP 조회, 웹폰트 금지. 모든 라이브러리·지도 데이터·GeoIP DB는 앱에 내장한다.
- Electron 금지. Tauri 2를 쓴다. 대상은 Windows 10(WebView2 설치돼 있음).
- 화면 문구는 모두 한국어. 숫자는 천 단위 쉼표. 글꼴은 `"Malgun Gothic", system-ui, sans-serif`.
- 날짜 경계와 모든 집계는 한국 시간(KST, +09:00) 기준. 로그에 시간대가 없으면 KST로 본다.
- 분류: `KR`은 국내, 그 외 국가는 해외, 사설·내부 대역은 "내부"(국내·해외 집계에서 제외), 판별 불가는 "알 수 없음"(해외에 넣지 않고 건수만 따로 표시).
- "전체 요청" = 국내 + 해외 요청(내부·알 수 없음 제외).
- "404를 낸 IP" = 선택 기간 안에 상태코드 404 응답이 1건 이상인 IP. 제외 스위치는 국내·해외 모두에 적용한다.
- 고유 IP는 사람 수가 아니다. 문구에 "명"을 쓰지 않고 "IP"를 쓴다.
- 보고서에 개조식(□, ○) 양식을 쓰지 않는다.
- 커밋 메시지는 한국어, 끝에 `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- 라이브러리 크레이트 이름은 `iptracker-core`(경로 `crates/core`). `core`라는 패키지 이름은 Rust 내장 크레이트를 가려 매크로가 깨지므로 쓰지 않는다.
- 앱 데이터(DB, 교체한 GeoIP)는 Windows 로밍 폴더가 아닌 로컬 폴더(`app_local_data_dir`)에 둔다.

**설계 문서와 다른 점 한 가지:** 설계 문서의 `country_hourly` 테이블은 만들지 않는다. 1단계 화면에 시간대별 차트가 없고, 국가 단위 요약으로는 "404를 낸 IP 제외"를 적용할 수 없기 때문이다. 일별 추이는 `ip_daily`에서 구한다. (설계 문서도 같이 수정함.)

## Review Focus

1. **IPv6·IPv4-mapped 주소**(`::ffff:1.2.3.4`, `2001:db8::1`): 파싱이 실패하지 않고, mapped 주소는 IPv4로 바꿔 분류한다. → Task 1, Task 2
2. **깨진 요청 줄**: 요청 필드가 `"-"`이거나 `"\x16\x03\x01"` 같은 바이너리 흔적, URL 안의 이스케이프된 따옴표(`\"`), UTF-8이 아닌 바이트. 줄을 버리지 않고 메서드·URL을 빈 문자열로 두어 집계하며, 파일 전체가 중단되지 않는다. → Task 1, Task 4
3. **잘린 gz**(쓰는 중에 받아온 압축 파일): 읽은 데까지 적재하지 않고 오류로 돌려주며, DB에는 그 파일의 흔적이 남지 않는다. → Task 4
4. **데이터 없는 기간**(첫 실행, 빈 DB, 적재 안 된 날짜): 모든 질의가 0을 돌려주고 화면에 `NaN`·`Infinity`가 나오지 않는다. → Task 5, Task 8
5. **원본 삭제 실패**(파일이 다른 프로그램에 잡혀 있음): 적재 결과는 유지하고, 삭제하지 못했다는 사실을 화면에 알린다. → Task 7

---

## File Structure

```
Cargo.toml                         워크스페이스(crates/core, crates/loggen, src-tauri)
crates/core/src/lib.rs             모듈 선언과 공통 Error
crates/core/src/logformat.rs       LogFormat 해석, 줄 파싱, 형식 자동 인식
crates/core/src/geo.rs             mmdb 로딩과 IP 분류
crates/core/src/db.rs              DB 열기, 스키마, 설정 값
crates/core/src/ingest.rs          파일 탐색, 미리보기, 적재, 대체, 정리
crates/core/src/query.rs           대시보드 집계 질의
crates/core/tests/e2e.rs           loggen → 적재 → 질의 통합 테스트
crates/loggen/src/{lib.rs,main.rs} 가짜 access 로그 생성기
src-tauri/                         Tauri 앱(명령, 상태, 설정 파일, 리소스)
src-tauri/resources/dbip-country-lite.mmdb
src/api.ts                         명령 타입과 invoke 래퍼
src/lib/{period,format,summary,countryName}.ts
src/screens/{Dashboard,Ingest,Settings,Report}.tsx
src/components/                    KpiTiles, WorldMap, TopCountries, DailyTrend, CountryTable, IpDrawer, Notices
src/assets/world.geo.json          Natural Earth 110m 국가 경계
scripts/check-offline.mjs          빌드 결과에 외부 URL이 없는지 검사
.github/workflows/release.yml      Windows 설치 파일 빌드
```

---

### Task 1: 로그 형식 해석과 줄 파싱 (`iptracker_core::logformat`)

**Files:**
- Create: `Cargo.toml`, `crates/core/Cargo.toml`, `crates/core/src/lib.rs`, `crates/core/src/logformat.rs`
- Modify: `.gitignore` (`target/`, `node_modules/`, `dist/` 추가)

**Interfaces:**
- Produces:
```rust
pub struct AccessRecord {
    pub ip: std::net::IpAddr,
    pub ts: chrono::DateTime<chrono::FixedOffset>,
    pub method: String,      // 요청 줄이 깨졌으면 ""
    pub url: String,         // 요청 줄이 깨졌으면 ""
    pub status: u16,
    pub bytes: Option<u64>,  // "-"이면 None
    pub user_agent: Option<String>,
}
pub const COMMON: &str = r#"%h %l %u %t "%r" %>s %b"#;
pub const COMBINED: &str = r#"%h %l %u %t "%r" %>s %b "%{Referer}i" "%{User-Agent}i""#;
pub struct LineParser { /* 비공개 */ }
impl LineParser {
    /// time_format은 chrono strftime 문자열. None이면 아래 TIME_FORMATS를 차례로 시도한다.
    pub fn new(log_format: &str, time_format: Option<&str>) -> Result<LineParser, FormatError>;
    pub fn parse(&self, line: &str) -> Option<AccessRecord>;
    pub fn log_format(&self) -> &str;
}
/// 비어 있지 않은 표본 줄로 형식을 고른다. 후보 중 성공률이 가장 높고 0.9 이상인 것.
/// time_format이 Some이면 모든 후보 LogFormat에 그 시간 형식을 쓴다.
pub fn detect(sample: &[String], time_format: Option<&str>) -> Option<LineParser>;
#[derive(Debug, thiserror::Error)] pub enum FormatError { MissingIp, MissingTime, MissingStatus }
```

**정해 둔 값:**
- 지원 지시자: `%h`·`%a`(클라이언트 IP, 형식에 먼저 나온 것 사용), `%t`, `%{형식}t`, `%r`, `%s`·`%>s`, `%b`·`%B`·`%O`, `%{User-Agent}i`. 그 밖의 `%...` 지시자는 값을 읽고 버린다.
- 형식 문자열에서 지시자가 아닌 문자(공백, `"`, `[`, `]` 등)는 리터럴이며 줄의 같은 위치 문자와 그대로 대조한다. 따옴표 리터럴 안의 필드는 `\"`를 이스케이프로 처리해 닫는 따옴표까지 읽는다. 그 밖의 필드는 형식상 다음 리터럴 문자가 나오기 전까지 읽는다(마지막 필드면 줄 끝까지).
- 시간: Apache는 `%t`를 `[22/Nov/2026:23:58:01 +0900]`처럼 **대괄호를 포함해** 출력하므로 `%t`는 `[`…`]`를 스스로 읽는다. `%{형식}t`는 대괄호 없이 출력되므로(실제 설정은 `[%{...}t]`처럼 대괄호를 리터럴로 씀) 대괄호를 읽지 않고, 중괄호 안의 형식을 그 필드의 시간 형식으로 쓴다(`time_format` 인자보다 우선). `%{형식}t` 값은 형식 안의 공백 수 + 1개의 공백 구분 덩어리를 읽는다.
- `parse`는 줄 끝의 `\r`을 먼저 떼어 낸다(Windows에서 저장한 CRLF 파일 대비).
- `TIME_FORMATS`(순서대로 시도): `%d/%b/%Y:%H:%M:%S %z`, `%d/%b/%Y:%H:%M:%S`, `%Y-%m-%d %H:%M:%S%.f`, `%Y-%m-%d %H:%M:%S,%3f`, `%Y-%m-%dT%H:%M:%S%z`, `%Y-%m-%dT%H:%M:%S`. 시간대가 없으면 `+09:00`.
- `detect` 후보: `COMBINED`, `COMMON`, `COMBINED + " %D"`, `"%v " + COMBINED`.
- `new`는 형식에 IP·시간·상태코드 지시자가 없으면 각각의 `FormatError`를 돌려준다.

- [ ] **Step 1: 실패하는 테스트 작성** (`logformat.rs`의 `#[cfg(test)] mod tests`)

```rust
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
```

- [ ] **Step 2: 실패 확인** — `cargo test -p iptracker-core logformat` → 컴파일 오류(`LineParser` 없음).
- [ ] **Step 3: 구현** — 워크스페이스와 `crates/core` 크레이트(패키지 이름 `iptracker-core`. Rust 내장 `core`를 가리므로 `core`라는 이름은 쓰지 않는다. 의존성 `chrono`, `thiserror`)를 만들고 위 인터페이스를 구현한다. `LogFormat` 문자열을 토큰 목록으로 한 번 컴파일하고, `parse`는 토큰을 따라 줄을 왼쪽에서 오른쪽으로 읽는다(정규식 미사용).
- [ ] **Step 4: 통과 확인** — `cargo test -p iptracker-core logformat` → 11 passed.
- [ ] **Step 5: 커밋** — `git add -A && git commit -m "access 로그 형식 해석과 줄 파싱 추가"`

---

### Task 2: 국가 판별 (`iptracker_core::geo`)과 GeoIP DB 내장

**Files:**
- Create: `crates/core/src/geo.rs`, `src-tauri/resources/dbip-country-lite.mmdb`, `src-tauri/resources/GEOIP-ATTRIBUTION.txt`
- Modify: `crates/core/src/lib.rs`, `crates/core/Cargo.toml`(`maxminddb`)

**Interfaces:**
- Produces:
```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)] pub enum OriginClass { Domestic = 0, Foreign = 1, Internal = 2, Unknown = 3 }
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Origin { pub class: OriginClass, pub country: Option<String> } // ISO 3166-1 alpha-2 대문자
pub struct GeoDb { /* 비공개 */ }
impl GeoDb {
    pub fn open(path: &std::path::Path) -> Result<GeoDb, crate::Error>;
    pub fn classify(&self, ip: std::net::IpAddr) -> Origin;
    pub fn build_date(&self) -> chrono::NaiveDate; // mmdb 메타데이터 build_epoch, KST 날짜
}
```

**정해 둔 값:**
- 내부 대역: `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `127.0.0.0/8`, `169.254.0.0/16`, `::1`, `fc00::/7`, `fe80::/10`. → `Origin { Internal, None }`.
- IPv4-mapped IPv6(`::ffff:a.b.c.d`)는 IPv4로 바꾼 뒤 분류한다.
- DB에 없거나 국가 코드가 비어 있으면 `Origin { Unknown, None }`.
- 국가 코드는 mmdb 레코드의 `country.iso_code`에서 읽는다.
- GeoIP 파일: `https://download.db-ip.com/free/dbip-country-lite-2026-11.mmdb.gz`를 받아 압축을 풀어 `src-tauri/resources/dbip-country-lite.mmdb`로 저장하고 git에 커밋한다. 해당 월 파일이 없으면 가장 최근 월 파일을 쓴다.
- `GEOIP-ATTRIBUTION.txt` 내용: `IP Geolocation by DB-IP (https://db-ip.com), licensed under CC BY 4.0.`

- [ ] **Step 1: GeoIP 파일을 내려받아 저장** — `curl -fL <위 URL> | gunzip > src-tauri/resources/dbip-country-lite.mmdb`, 크기가 1MB 이상인지 확인.
- [ ] **Step 2: 실패하는 테스트 작성**

```rust
fn db() -> GeoDb {
    GeoDb::open(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src-tauri/resources/dbip-country-lite.mmdb"))).unwrap()
}
fn c(ip: &str) -> Origin { db().classify(ip.parse().unwrap()) }

#[test] fn korean_ip_is_domestic() { assert_eq!(c("168.126.63.1"), Origin { class: OriginClass::Domestic, country: Some("KR".into()) }); }
#[test] fn us_ip_is_foreign() { assert_eq!(c("8.8.8.8"), Origin { class: OriginClass::Foreign, country: Some("US".into()) }); }
#[test] fn private_ranges_are_internal() {
    for ip in ["10.1.2.3", "172.16.0.1", "172.31.255.254", "192.168.0.10", "127.0.0.1", "169.254.1.1", "::1", "fd00::1", "fe80::1"] {
        assert_eq!(c(ip), Origin { class: OriginClass::Internal, country: None }, "{ip}");
    }
}
#[test] fn public_172_is_not_internal() { assert_ne!(c("172.32.0.1").class, OriginClass::Internal); }
#[test] fn mapped_ipv6_uses_ipv4() { assert_eq!(c("::ffff:8.8.8.8"), c("8.8.8.8")); assert_eq!(c("::ffff:10.0.0.1").class, OriginClass::Internal); }
#[test] fn build_date_is_recent() { assert!(db().build_date() >= chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()); }
#[test] fn open_missing_file_is_error() { assert!(GeoDb::open(std::path::Path::new("/nonexistent.mmdb")).is_err()); }
```

- [ ] **Step 3: 실패 확인** — `cargo test -p iptracker-core geo` → 컴파일 오류.
- [ ] **Step 4: 구현** — `maxminddb::Reader`로 파일을 메모리에 읽어 들인다. `crate::Error`(`thiserror`, `Io`/`Geo`/`Db`/`Other(String)` 변형)를 `lib.rs`에 정의한다.
- [ ] **Step 5: 통과 확인** — `cargo test -p iptracker-core geo` → 7 passed.
- [ ] **Step 6: 커밋** — `git add -A && git commit -m "GeoIP DB 내장과 IP 국가 분류 추가"`

---

### Task 3: DB 스키마와 설정 값 (`iptracker_core::db`)

**Files:**
- Create: `crates/core/src/db.rs`
- Modify: `crates/core/src/lib.rs`, `crates/core/Cargo.toml`(`rusqlite` features `bundled`)

**Interfaces:**
- Produces:
```rust
pub fn open(path: &std::path::Path) -> Result<rusqlite::Connection, crate::Error>; // 없으면 만들고 최신 스키마까지 올림
pub fn open_in_memory() -> Result<rusqlite::Connection, crate::Error>;            // 테스트용
pub fn get_setting(conn: &rusqlite::Connection, key: &str) -> Result<Option<String>, crate::Error>;
pub fn set_setting(conn: &rusqlite::Connection, key: &str, value: &str) -> Result<(), crate::Error>;
/// PRAGMA incremental_vacuum 후 wal_checkpoint(TRUNCATE). 삭제·대체·적재 뒤에 호출해 파일 크기를 실제로 줄인다.
pub fn compact(conn: &rusqlite::Connection) -> Result<(), crate::Error>;
/// VACUUM INTO로 현재 DB를 dest에 복사한다(WAL 내용 포함). dest가 이미 있으면 오류.
pub fn copy_to(conn: &rusqlite::Connection, dest: &std::path::Path) -> Result<(), crate::Error>;
pub const SCHEMA_VERSION: i32 = 1;
```

**스키마(버전 1, 그대로 사용):**
```sql
CREATE TABLE ingest_files(
  id INTEGER PRIMARY KEY, server TEXT NOT NULL, filename TEXT NOT NULL, sha256 TEXT NOT NULL,
  date_from TEXT, date_to TEXT, lines INTEGER NOT NULL, failed_lines INTEGER NOT NULL,
  ingested_at INTEGER NOT NULL, UNIQUE(server, filename));
CREATE INDEX idx_files_sha ON ingest_files(sha256);
CREATE TABLE ip_daily(
  date TEXT NOT NULL, file_id INTEGER NOT NULL REFERENCES ingest_files(id) ON DELETE CASCADE,
  ip TEXT NOT NULL, class INTEGER NOT NULL, country TEXT,
  requests INTEGER NOT NULL, not_found INTEGER NOT NULL,
  first_ts INTEGER NOT NULL, last_ts INTEGER NOT NULL,
  PRIMARY KEY(date, file_id, ip)) WITHOUT ROWID;
CREATE INDEX idx_ip_daily_ip ON ip_daily(ip, date);
CREATE INDEX idx_ip_daily_file ON ip_daily(file_id);
CREATE TABLE user_agents(id INTEGER PRIMARY KEY, ua TEXT NOT NULL UNIQUE);
CREATE TABLE foreign_requests(
  id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, date TEXT NOT NULL, ip TEXT NOT NULL, country TEXT NOT NULL,
  file_id INTEGER NOT NULL REFERENCES ingest_files(id) ON DELETE CASCADE,
  method TEXT NOT NULL, url TEXT NOT NULL, status INTEGER NOT NULL, bytes INTEGER,
  ua_id INTEGER REFERENCES user_agents(id));
CREATE INDEX idx_fr_ip_ts ON foreign_requests(ip, ts);
CREATE INDEX idx_fr_file ON foreign_requests(file_id);
CREATE INDEX idx_fr_date ON foreign_requests(date);
CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
```
- `date`는 KST 기준 `YYYY-MM-DD`, `ts`류는 유닉스 초(UTC), `class`는 `OriginClass` 값, `ip`는 `IpAddr::to_string()`(IPv4-mapped는 IPv4 표기).
- 새 DB를 만들 때는 **테이블을 만들기 전에** `PRAGMA auto_vacuum=INCREMENTAL`을 켠다(나중에는 켤 수 없음. 이게 없으면 오래된 상세를 지워도 파일이 줄지 않는다).
- 열 때마다 `PRAGMA foreign_keys=ON`, `PRAGMA journal_mode=WAL`. 버전은 `PRAGMA user_version`으로 관리하고, 버전별 마이그레이션 SQL 배열을 순서대로 적용한다(2·3단계에서 항목을 추가하는 구조).

- [ ] **Step 1: 실패하는 테스트 작성**

```rust
#[test] fn fresh_db_is_at_latest_version_with_all_tables() {
    let c = open_in_memory().unwrap();
    let v: i32 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(v, SCHEMA_VERSION);
    for t in ["ingest_files", "ip_daily", "user_agents", "foreign_requests", "settings"] {
        let n: i32 = c.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1", [t], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "{t}");
    }
}
#[test] fn reopening_keeps_data() {
    let dir = tempfile::tempdir().unwrap(); let p = dir.path().join("a.db");
    { let c = open(&p).unwrap(); set_setting(&c, "service_name", "테스트").unwrap(); }
    let c = open(&p).unwrap();
    assert_eq!(get_setting(&c, "service_name").unwrap().as_deref(), Some("테스트"));
}
#[test] fn missing_setting_is_none_and_set_overwrites() {
    let c = open_in_memory().unwrap();
    assert_eq!(get_setting(&c, "x").unwrap(), None);
    set_setting(&c, "x", "1").unwrap(); set_setting(&c, "x", "2").unwrap();
    assert_eq!(get_setting(&c, "x").unwrap().as_deref(), Some("2"));
}
#[test] fn new_db_uses_incremental_auto_vacuum() {
    let dir = tempfile::tempdir().unwrap();
    let c = open(&dir.path().join("a.db")).unwrap();
    let v: i32 = c.query_row("PRAGMA auto_vacuum", [], |r| r.get(0)).unwrap();
    assert_eq!(v, 2);
}
#[test] fn copy_to_includes_uncheckpointed_data() {
    let dir = tempfile::tempdir().unwrap();
    let c = open(&dir.path().join("a.db")).unwrap();
    set_setting(&c, "k", "v").unwrap();
    copy_to(&c, &dir.path().join("b.db")).unwrap();
    assert_eq!(get_setting(&open(&dir.path().join("b.db")).unwrap(), "k").unwrap().as_deref(), Some("v"));
    assert!(copy_to(&c, &dir.path().join("b.db")).is_err());
}
#[test] fn deleting_a_file_cascades() {
    let c = open_in_memory().unwrap();
    c.execute("INSERT INTO ingest_files(id,server,filename,sha256,lines,failed_lines,ingested_at) VALUES(1,'s','f','h',1,0,0)", []).unwrap();
    c.execute("INSERT INTO ip_daily VALUES('2026-11-22',1,'1.1.1.1',1,'US',1,0,0,0)", []).unwrap();
    c.execute("DELETE FROM ingest_files WHERE id=1", []).unwrap();
    let n: i32 = c.query_row("SELECT count(*) FROM ip_daily", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 0);
}
```

- [ ] **Step 2: 실패 확인** — `cargo test -p iptracker-core db` → 컴파일 오류.
- [ ] **Step 3: 구현** (dev-dependency `tempfile`).
- [ ] **Step 4: 통과 확인** — `cargo test -p iptracker-core db` → 6 passed.
- [ ] **Step 5: 커밋** — `git add -A && git commit -m "SQLite 스키마와 설정 저장 추가"`

---

### Task 4: 적재 (`iptracker_core::ingest`)

**Files:**
- Create: `crates/core/src/ingest.rs`
- Modify: `crates/core/src/lib.rs`, `crates/core/Cargo.toml`(`flate2`, `sha2`, `regex`, `walkdir`, `serde`)

**Interfaces:**
- Consumes: `LineParser`, `detect`(Task 1), `GeoDb`, `OriginClass`(Task 2), `db::*`(Task 3)
- Produces(모두 `serde::Serialize`/`Deserialize`. 구조체는 `#[serde(rename_all = "camelCase")]`, enum은 아래처럼 `rename_all_fields`까지 붙여 필드 이름도 camelCase로 나가게 한다):
```rust
pub struct Candidate { pub path: PathBuf, pub server: String, pub filename: String } // filename은 끝의 ".gz"를 뗀 이름
/// 파일과 폴더(하위 포함)를 받아 일반 파일 목록을 경로순으로 돌려준다.
/// server: server_regex가 있고 파일명에 맞으면 첫 번째 캡처 그룹, 아니면 바로 위 폴더 이름.
pub fn scan(paths: &[PathBuf], server_regex: Option<&regex::Regex>) -> Vec<Candidate>;

pub struct PreviewRow { pub ip: String, pub time: String /* "YYYY-MM-DD HH:MM:SS" KST */, pub method: String, pub url: String, pub status: u16 }
pub struct Preview { pub log_format: Option<String>, pub rows: Vec<PreviewRow>, pub sampled: usize, pub failed: usize }
/// 비어 있지 않은 앞 20줄을 읽는다. parser가 None이면 detect로 고르고, 못 고르면 log_format=None, rows는 빈 목록.
pub fn preview(path: &Path, parser: Option<&LineParser>) -> Result<Preview, crate::Error>;
/// 파일 앞 50줄로 형식을 자동 인식한다.
pub fn detect_file(path: &Path, time_format: Option<&str>) -> Result<Option<LineParser>, crate::Error>;

#[serde(rename_all = "camelCase")] pub enum ConflictPolicy { Ask, Replace, Skip } // "ask" | "replace" | "skip"
pub struct IngestOptions { pub conflict: ConflictPolicy, pub allow_high_failure: bool }
pub struct FileSummary { pub file_id: i64, pub lines: u64, pub failed_lines: u64, pub foreign_requests: u64, pub date_from: Option<String>, pub date_to: Option<String> }
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum IngestOutcome {
    Ingested(FileSummary),
    Duplicate,                 // 같은 내용(sha256)이 이미 적재됨
    Conflict,                  // 같은 server+filename이 다른 내용으로 적재돼 있고 정책이 Ask
    Skipped,                   // Conflict인데 정책이 Skip
    HighFailure { lines: u64, failed_lines: u64, samples: Vec<String> }, // 실패율 1% 초과, allow_high_failure=false
}
pub fn ingest_file(conn: &mut rusqlite::Connection, geo: &GeoDb, parser: &LineParser, c: &Candidate,
                   opts: &IngestOptions, progress: &mut dyn FnMut(u64, u64)) -> Result<IngestOutcome, crate::Error>;
/// date(YYYY-MM-DD) 이전의 foreign_requests를 지우고 지운 행 수를 돌려준다. ip_daily는 건드리지 않는다.
/// 지운 뒤 db::compact를 호출한다.
pub fn purge_foreign_before(conn: &rusqlite::Connection, date: &str) -> Result<usize, crate::Error>;
```

**정해 둔 동작:**
- 파일명이 `.gz`로 끝나면 gzip으로 풀어 읽는다. `sha256`은 **풀린 내용** 기준이다(같은 로그의 gz본과 원본은 같은 파일로 본다).
- 줄은 바이트로 읽어 `String::from_utf8_lossy`로 바꾼다. 빈 줄은 줄 수에 넣지 않는다.
- 두 번 읽는다. **1차**: 파싱 없이 내용의 `sha256`과 비어 있지 않은 줄 수만 구한다. 이것으로 판정한다: 줄 수가 0이 아니고 같은 `sha256`이 있으면 `Duplicate` → 같은 `server+filename`이 있으면 정책에 따라 `Conflict`/`Skipped`(여기까지 DB를 건드리지 않음). **2차**: 트랜잭션을 열고, 대체라면 기존 `ingest_files` 행을 지운 뒤(연쇄 삭제) 새 `ingest_files` 행을 먼저 넣어 `file_id`를 얻고, 파싱하면서 `foreign_requests`를 바로 넣는다. 끝나면 `failed_lines / lines > 0.01`이고 `allow_high_failure`가 false일 때 롤백 후 `HighFailure`(`samples`는 실패한 줄 앞에서 5개), 아니면 `ip_daily`와 `ingest_files`의 집계 값을 넣고 커밋.
- 오류(I/O, 잘린 gz 포함)가 나면 롤백하고 `Err`을 돌려준다.
- 빈 파일(0줄)은 해시 중복 검사에서 뺀다(빈 파일끼리는 해시가 같으므로).
- 줄 끝의 `\r`은 떼고 처리한다(해시는 원래 바이트 기준).
- `ip_daily`: 모든 분류의 IP를 (KST 날짜, IP)별로 메모리에서 합산해 넣는다. `not_found`는 상태코드 404 건수.
- `foreign_requests`: `class == Foreign`인 요청만 한 줄씩 넣는다. User-Agent는 `user_agents`에 넣고 번호로 참조한다.
- IP 문자열: IPv4-mapped IPv6는 IPv4 표기로 저장한다.
- `progress(처리량, 전체)`: 전체는 디스크 파일 크기의 2배(1차·2차 읽기). 약 1MB마다 호출하고, 결과가 무엇이든 끝에 `(전체, 전체)`를 한 번 호출한다.
- `lines == 0`인 파일(빈 파일)은 `HighFailure`가 아니라 `Ingested`(0줄)로 처리한다.

- [ ] **Step 1: 테스트 도우미와 실패하는 테스트 작성** — 테스트 모듈에 `fn write(dir, rel_path, lines: &[String]) -> PathBuf`, `fn write_gz(...)`, `fn line(ip, "22/Nov/2026:23:59:59 +0900", "GET /a", status) -> String`(combined 형식), `fn setup() -> (Connection, GeoDb, LineParser)`를 둔다. 테스트 IP: 국내 `168.126.63.1`, 미국 `8.8.8.8`, 내부 `10.0.0.1`.

```rust
#[test] fn scan_uses_parent_folder_as_server_and_strips_gz() {
    // dir/was01/access.2026-11-22, dir/was02/access.2026-11-22.gz
    let cs = scan(&[dir.path().to_path_buf()], None);
    assert_eq!(cs.iter().map(|c| (c.server.as_str(), c.filename.as_str())).collect::<Vec<_>>(),
               vec![("was01", "access.2026-11-22"), ("was02", "access.2026-11-22")]);
}
#[test] fn scan_server_regex_wins_when_it_matches() {
    // dir/logs/was07_access.log, dir/logs/other.log
    let re = regex::Regex::new(r"^(was\d+)_").unwrap();
    let cs = scan(&[dir.path().to_path_buf()], Some(&re));
    assert_eq!(cs.iter().map(|c| c.server.as_str()).collect::<Vec<_>>(), vec!["logs", "was07"]); // 경로순: other.log, was07_access.log
}
#[test] fn preview_shows_first_20_rows_in_kst() {
    // 30줄짜리 파일
    let p = preview(&path, None).unwrap();
    assert_eq!((p.rows.len(), p.sampled, p.failed), (20, 20, 0));
    assert_eq!(p.log_format.as_deref(), Some(COMBINED));
    assert_eq!(p.rows[0].time, "2026-11-22 23:59:59");
}
#[test] fn preview_of_unrecognized_file_has_no_format() {
    let p = preview(&garbage_path, None).unwrap();
    assert!(p.log_format.is_none() && p.rows.is_empty());
}
#[test] fn ingest_aggregates_all_ips_and_keeps_foreign_rows_only() {
    // 국내 IP 3줄(그중 404 한 줄), 미국 IP 2줄(그중 404 한 줄), 내부 IP 1줄
    let IngestOutcome::Ingested(s) = ingest_file(..., &ask(), &mut |_, _| {}).unwrap() else { panic!() };
    assert_eq!((s.lines, s.failed_lines, s.foreign_requests), (6, 0, 2));
    assert_eq!((s.date_from.as_deref(), s.date_to.as_deref()), (Some("2026-11-22"), Some("2026-11-22")));
    // ip_daily: (168.126.63.1, class 0, 'KR', requests 3, not_found 1), (8.8.8.8, 1, 'US', 2, 1), (10.0.0.1, 2, NULL, 1, 0)
    // foreign_requests: 2행, 둘 다 ip='8.8.8.8', country='US'
    // user_agents: 같은 UA는 1행
}
#[test] fn date_boundary_follows_kst() {
    // "22/Nov/2026:15:30:00 +0000" (UTC) 한 줄 → KST로는 11/23 00:30
    assert_eq!(dates_in_ip_daily(&conn), vec!["2026-11-23"]);
}
#[test] fn gz_and_plain_of_same_content_are_duplicates() {
    assert!(matches!(ingest(plain), IngestOutcome::Ingested(_)));
    assert!(matches!(ingest(gz_same_content_other_server), IngestOutcome::Duplicate));
    assert_eq!(count(&conn, "ingest_files"), 1);
}
#[test] fn same_name_different_content_asks_then_replaces_exactly() {
    ingest(v1 /* was01/access.log: 미국 IP 2줄 */);
    assert!(matches!(ingest_with(v2 /* 같은 경로: 미국 IP 5줄 */, ConflictPolicy::Ask), IngestOutcome::Conflict));
    assert!(matches!(ingest_with(v2, ConflictPolicy::Skip), IngestOutcome::Skipped));
    assert_eq!(sum_requests(&conn), 2);
    assert!(matches!(ingest_with(v2, ConflictPolicy::Replace), IngestOutcome::Ingested(_)));
    assert_eq!((sum_requests(&conn), count(&conn, "foreign_requests"), count(&conn, "ingest_files")), (5, 5, 1));
}
#[test] fn split_files_of_same_day_both_ingest() {
    // was01/access.2026-11-22 과 was01/access.2026-11-22_1, 내용 다름
    assert_eq!(count(&conn, "ingest_files"), 2);
}
#[test] fn high_failure_rolls_back_until_allowed() {
    // 정상 90줄 + 깨진 10줄
    let IngestOutcome::HighFailure { lines, failed_lines, samples } = ingest(f) else { panic!() };
    assert_eq!((lines, failed_lines, samples.len()), (100, 10, 5));
    assert_eq!(count(&conn, "ingest_files"), 0);
    assert!(matches!(ingest_allowing(f), IngestOutcome::Ingested(_)));
}
#[test] fn failure_at_or_below_one_percent_is_accepted() {
    // 정상 99줄 + 깨진 1줄 → Ingested, failed_lines == 1
}
#[test] fn non_utf8_bytes_do_not_abort_the_file() {
    // 둘째 줄 URL에 0xFF 0xFE 바이트가 섞인 3줄 파일
    let IngestOutcome::Ingested(s) = ingest(f) else { panic!() };
    assert_eq!((s.lines, s.failed_lines), (3, 0));
}
#[test] fn truncated_gz_is_an_error_and_leaves_nothing() {
    // 정상 gz의 뒤 절반을 잘라낸 파일
    assert!(ingest_result(f).is_err());
    assert_eq!((count(&conn, "ingest_files"), count(&conn, "ip_daily")), (0, 0));
}
#[test] fn empty_file_ingests_as_zero_lines() {
    let IngestOutcome::Ingested(s) = ingest(empty) else { panic!() };
    assert_eq!((s.lines, s.date_from), (0, None));
}
#[test] fn two_empty_files_are_not_duplicates() {
    // was01/a.log, was02/b.log 둘 다 빈 파일 → 둘 다 Ingested
}
#[test] fn crlf_file_parses_like_lf() {
    // 같은 3줄을 \r\n으로 저장 → Ingested, failed_lines 0, foreign_requests의 user_agent 끝에 \r 없음
}
#[test] fn duplicate_is_decided_before_any_db_write() {
    // 같은 파일을 두 번째 넣을 때 ingest_files·ip_daily·foreign_requests 행 수가 처음과 같고, 결과 Duplicate
}
#[test] fn serde_shape_is_camel_case() {
    let v = serde_json::to_value(IngestOutcome::HighFailure { lines: 100, failed_lines: 10, samples: vec![] }).unwrap();
    assert_eq!(v, serde_json::json!({"kind": "highFailure", "lines": 100, "failedLines": 10, "samples": []}));
    let v = serde_json::to_value(IngestOutcome::Ingested(FileSummary { file_id: 1, lines: 2, failed_lines: 0, foreign_requests: 1, date_from: None, date_to: None })).unwrap();
    assert_eq!(v["kind"], "ingested"); assert_eq!(v["fileId"], 1); assert_eq!(v["foreignRequests"], 1);
}
#[test] fn mapped_ipv6_is_stored_as_ipv4() { /* "::ffff:8.8.8.8" 한 줄 → ip_daily.ip == "8.8.8.8" */ }
#[test] fn purge_removes_old_foreign_rows_but_keeps_summary() {
    // 11/20, 11/22 미국 요청 각 1줄 적재 후
    assert_eq!(purge_foreign_before(&conn, "2026-11-21").unwrap(), 1);
    assert_eq!((count(&conn, "foreign_requests"), count(&conn, "ip_daily")), (1, 2));
}
#[test] fn progress_reaches_total() { /* 마지막 호출의 두 값이 같고 파일 크기의 2배다. Duplicate일 때도 마지막 호출은 (전체, 전체) */ }
```

- [ ] **Step 2: 실패 확인** — `cargo test -p iptracker-core ingest` → 컴파일 오류.
- [ ] **Step 3: 구현** (dev-dependency `serde_json`) — 한 줄씩 읽는 스트리밍 방식. 수 GB 파일을 메모리에 올리지 않는다. `foreign_requests` 삽입은 prepared statement를 재사용한다.
- [ ] **Step 4: 통과 확인** — `cargo test -p iptracker-core ingest` → 21 passed.
- [ ] **Step 5: 커밋** — `git add -A && git commit -m "로그 파일 탐색·미리보기·적재 추가"`

---

### Task 5: 집계 질의 (`iptracker_core::query`)

**Files:**
- Create: `crates/core/src/query.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: 스키마(Task 3)
- Produces(모두 `Serialize`/`Deserialize`, camelCase):
```rust
pub struct Period { pub from: String, pub to: String }      // "YYYY-MM-DD", 양끝 포함
pub struct Filter { pub period: Period, pub exclude_404: bool }
pub struct PeriodStats { pub total_requests: u64, pub foreign_requests: u64, pub foreign_ips: u64, pub domestic_ips: u64, pub countries: u64 }
pub struct Kpis {
    pub cur: PeriodStats,
    pub prev: PeriodStats,          // 바로 앞의 같은 길이 기간
    pub foreign_ips_404: u64,       // 스위치와 무관하게 센, 404를 낸 해외 IP 수
    pub excluded_ips: u64,          // 스위치와 무관하게 센, 404를 낸 국내+해외 IP 수
    pub new_countries: u64,         // prev에는 없고 cur에는 있는 해외 국가 수
    pub unknown_requests: u64,      // 분류 "알 수 없음" 요청 수
}
pub fn kpis(conn: &Connection, f: &Filter) -> Result<Kpis, crate::Error>;

pub struct CountryRow { pub country: String, pub requests: u64, pub share: f64 /* 해외 요청 중 비율 0~1 */, pub ips: u64, pub not_found: u64, pub ips_404: u64, pub last_ts: i64 }
pub fn countries(conn: &Connection, f: &Filter) -> Result<Vec<CountryRow>, crate::Error>; // 해외만, 요청 수 내림차순

pub struct DayPoint { pub date: String, pub foreign_requests: u64, pub foreign_ips: u64 }
pub fn daily(conn: &Connection, f: &Filter) -> Result<Vec<DayPoint>, crate::Error>;       // 기간의 모든 날짜, 데이터 없으면 0

pub struct IpRow { pub ip: String, pub requests: u64, pub not_found: u64, pub first_ts: i64, pub last_ts: i64 }
pub fn country_ips(conn: &Connection, f: &Filter, country: &str, limit: u32, offset: u32) -> Result<Vec<IpRow>, crate::Error>; // 요청 수 내림차순

pub struct RequestRow { pub ts: i64, pub method: String, pub url: String, pub status: u16, pub bytes: Option<u64>, pub user_agent: Option<String>, pub server: String }
pub fn ip_requests(conn: &Connection, period: &Period, ip: &str, limit: u32, offset: u32) -> Result<Vec<RequestRow>, crate::Error>; // 시간 내림차순

pub struct IngestStatus { pub latest_date: Option<String>, pub servers_on_latest: u32, pub file_count: u32 }
pub fn ingest_status(conn: &Connection) -> Result<IngestStatus, crate::Error>;
```

**정해 둔 동작:**
- `exclude_404`가 true면, **그 기간 안에서** `not_found` 합이 1 이상인 IP의 모든 행을 계산에서 뺀다(국내·해외 공통). `prev` 통계는 prev 기간 자체의 404 IP 집합으로 뺀다.
- `total_requests` = class 0 + class 1 요청. 내부(2)와 알 수 없음(3)은 어디에도 넣지 않는다.
- 고유 IP는 기간 내 `COUNT(DISTINCT ip)`(여러 파일·서버·날짜에 걸친 같은 IP는 1개).
- `share`는 해외 요청이 0이면 0.0.
- `ingest_status`: `latest_date`는 `ip_daily`의 최대 날짜, `servers_on_latest`는 그 날짜에 행이 있는 파일들의 서로 다른 `server` 수.

- [ ] **Step 1: 실패하는 테스트 작성** — `fn seed(conn, rows: &[(&str /*date*/, &str /*server*/, &str /*ip*/, u8 /*class*/, Option<&str>, u64 /*requests*/, u64 /*not_found*/)])` 도우미는 서버마다 `ingest_files` 행을 하나 만들고 `ip_daily`에 직접 넣는다. 기본 데이터 `BASE`:

```
("2026-11-22","was01","168.126.63.1",0,Some("KR"),100,0)
("2026-11-22","was02","168.126.63.1",0,Some("KR"), 50,0)   // 같은 국내 IP, 다른 서버
("2026-11-22","was01","168.126.63.2",0,Some("KR"), 10,2)   // 404를 낸 국내 IP
("2026-11-22","was01","8.8.8.8",     1,Some("US"), 30,0)
("2026-11-21","was01","8.8.8.8",     1,Some("US"), 10,0)   // 같은 해외 IP, 다른 날
("2026-11-22","was01","1.0.0.1",     1,Some("JP"),  5,5)   // 404만 낸 해외 IP
("2026-11-22","was01","10.0.0.1",    2,None,      999,0)   // 내부
("2026-11-22","was01","203.0.113.1", 3,None,        7,0)   // 알 수 없음
("2026-11-19","was01","9.9.9.9",     1,Some("DE"),  4,0)   // 직전 기간(11/19~11/20)
```
기간 `P` = 11/21~11/22.

```rust
#[test] fn kpis_without_exclusion() {
    let k = kpis(&c, &f(P, false)).unwrap();
    assert_eq!(k.cur, PeriodStats { total_requests: 205, foreign_requests: 45, foreign_ips: 2, domestic_ips: 2, countries: 2 });
    assert_eq!(k.prev, PeriodStats { total_requests: 4, foreign_requests: 4, foreign_ips: 1, domestic_ips: 0, countries: 1 });
    assert_eq!((k.foreign_ips_404, k.excluded_ips, k.new_countries, k.unknown_requests), (1, 2, 2, 7));
}
#[test] fn kpis_with_exclusion_drops_404_ips_on_both_sides() {
    let k = kpis(&c, &f(P, true)).unwrap();
    assert_eq!(k.cur, PeriodStats { total_requests: 190, foreign_requests: 40, foreign_ips: 1, domestic_ips: 1, countries: 1 });
    assert_eq!((k.foreign_ips_404, k.excluded_ips), (1, 2)); // 스위치와 무관
}
#[test] fn countries_rows() {
    let v = countries(&c, &f(P, false)).unwrap();
    assert_eq!(v.iter().map(|r| (r.country.as_str(), r.requests, r.ips, r.not_found, r.ips_404)).collect::<Vec<_>>(),
               vec![("US", 40, 1, 0, 0), ("JP", 5, 1, 5, 1)]);
    assert!((v[0].share - 40.0 / 45.0).abs() < 1e-9);
    assert_eq!(countries(&c, &f(P, true)).unwrap().len(), 1);
}
#[test] fn daily_fills_missing_dates_with_zero() {
    let v = daily(&c, &f(period("2026-11-20", "2026-11-22"), false)).unwrap();
    assert_eq!(v.iter().map(|d| (d.date.as_str(), d.foreign_requests, d.foreign_ips)).collect::<Vec<_>>(),
               vec![("2026-11-20", 0, 0), ("2026-11-21", 10, 1), ("2026-11-22", 35, 2)]);
}
#[test] fn country_ips_merges_days_and_servers() {
    let v = country_ips(&c, &f(P, false), "US", 50, 0).unwrap();
    assert_eq!((v.len(), v[0].ip.as_str(), v[0].requests), (1, "8.8.8.8", 40));
}
#[test] fn empty_db_returns_zeros() {
    let c = db::open_in_memory().unwrap();
    let k = kpis(&c, &f(P, true)).unwrap();
    assert_eq!(k.cur, PeriodStats { total_requests: 0, foreign_requests: 0, foreign_ips: 0, domestic_ips: 0, countries: 0 });
    assert!(countries(&c, &f(P, false)).unwrap().is_empty());
    assert_eq!(daily(&c, &f(P, false)).unwrap().len(), 2);
    let s = ingest_status(&c).unwrap();
    assert_eq!((s.latest_date, s.servers_on_latest, s.file_count), (None, 0, 0));
}
#[test] fn ingest_status_counts_servers_on_latest_date() {
    let s = ingest_status(&c).unwrap(); // BASE
    assert_eq!((s.latest_date.as_deref(), s.servers_on_latest), (Some("2026-11-22"), 2));
}
#[test] fn ip_requests_are_newest_first_with_server_name() {
    // foreign_requests에 8.8.8.8 두 건(ts 100, 200)을 was01 파일로 직접 넣는다
    let v = ip_requests(&c, &P, "8.8.8.8", 10, 0).unwrap();
    assert_eq!(v.iter().map(|r| (r.ts, r.server.as_str())).collect::<Vec<_>>(), vec![(200, "was01"), (100, "was01")]);
}
```

- [ ] **Step 2: 실패 확인** — `cargo test -p iptracker-core query` → 컴파일 오류.
- [ ] **Step 3: 구현** — 404 IP 집합은 `WITH bad AS (SELECT ip FROM ip_daily WHERE date BETWEEN ?1 AND ?2 AND class IN (0,1) GROUP BY ip HAVING SUM(not_found) > 0)` 형태의 CTE로 공통 처리한다.
- [ ] **Step 4: 통과 확인** — `cargo test -p iptracker-core query` → 8 passed.
- [ ] **Step 5: 커밋** — `git add -A && git commit -m "대시보드 집계 질의 추가"`

---

### Task 6: 가짜 로그 생성기(`loggen`)와 통합 테스트

**Files:**
- Create: `crates/loggen/Cargo.toml`, `crates/loggen/src/lib.rs`, `crates/loggen/src/main.rs`, `crates/core/tests/e2e.rs`
- Modify: `Cargo.toml`(워크스페이스 멤버), `crates/core/Cargo.toml`(dev-dependency `loggen`)

**Interfaces:**
- Produces:
```rust
pub struct GenConfig { pub out_dir: PathBuf, pub servers: u32, pub days: Vec<chrono::NaiveDate>, pub requests_per_server_day: u32,
                       pub foreign_ratio: f64, pub scanner_ips: u32, pub gzip: bool, pub seed: u64 }
pub struct GenReport { pub files: Vec<PathBuf>, pub total_lines: u64, pub foreign_lines: u64, pub not_found_ips: u64 }
pub fn generate(cfg: &GenConfig) -> std::io::Result<GenReport>;
pub const COUNTRY_PREFIXES: &[(&str, &str)]; // (ISO2, /24 접두사 "a.b.c") — IP는 반드시 이 /24 안에서만 만든다
```
- CLI: `cargo run -p loggen -- --out <dir> [--servers 16] [--days 7] [--end 2026-11-22] [--per-day 20000] [--foreign 0.02] [--scanners 30] [--gzip] [--seed 1]`

**정해 둔 동작:**
- 출력: `<out>/was01/access_log.2026-11-22` … `<out>/was16/…`, combined 형식, 시간은 `+0900`, 파일 안에서 시간순.
- `--gzip`이면 마지막 날을 뺀 날짜의 파일을 `.gz`로 쓴다.
- 국내 IP는 KR 접두사에서, 해외 IP는 `COUNTRY_PREFIXES`의 12개국 이상(US, JP, CN, VN, DE, CA, GB, FR, SG, AU, IN, BR 포함) 대역에서 뽑는다. 미국·일본·중국 비중이 크게.
- 한 IP의 요청은 여러 서버에 고르게 나눠 쓴다(실제 L4 분배처럼. DB 크기 실측이 현실적이 되도록).
- 정상 요청은 `/api/v1/...` 경로 몇 개에 200. "스캐너" IP(해외 위주, 일부 국내)는 `/wp-login.php`, `/.env`, `/phpmyadmin/` 같은 경로에 404만 낸다.
- 서버마다 `10.0.0.1`의 상태 점검 요청(`GET /health`)을 조금 섞는다.
- 같은 `seed`면 결과가 같다(`rand`의 시드 고정 생성기).

- [ ] **Step 1: 실패하는 테스트 작성**

`crates/loggen/src/lib.rs`:
```rust
#[test] fn same_seed_same_output() { /* 두 번 생성한 파일 내용이 바이트 단위로 같다 */ }
#[test] fn layout_is_one_folder_per_server() { /* servers=3, days=2 → was01..was03 폴더에 파일 2개씩 */ }
```
`crates/core/tests/e2e.rs` (`loggen`은 `core`에 의존하지 않는다. 두 크레이트를 함께 쓰는 검사는 모두 여기에 둔다):
```rust
#[test] fn country_samples_match_bundled_geoip() {
    // loggen::COUNTRY_PREFIXES의 모든 /24에 대해 .1과 .254가 내장 mmdb에서 그 국가로 분류된다
}
#[test] fn generated_logs_ingest_and_query_consistently() {
    // generate(servers 4, days [11/21, 11/22], per-day 500, foreign 0.1, scanners 5, gzip true, seed 7)
    // scan → detect_file → 모든 파일 ingest_file
    let k = kpis(&conn, &Filter { period: p("2026-11-21", "2026-11-22"), exclude_404: false }).unwrap();
    assert_eq!(count(&conn, "ingest_files"), 8);
    assert_eq!(count(&conn, "foreign_requests") as u64, report.foreign_lines);
    assert_eq!(k.cur.foreign_requests, report.foreign_lines);
    assert_eq!(k.excluded_ips, report.not_found_ips);
    assert!(k.cur.countries >= 3);
    // 같은 폴더를 다시 넣으면 전부 Duplicate, 숫자 변화 없음
    let k_ex = kpis(&conn, &Filter { period: p("2026-11-21", "2026-11-22"), exclude_404: true }).unwrap();
    assert!(k_ex.cur.total_requests < k.cur.total_requests);
    assert_eq!(ingest_status(&conn).unwrap().servers_on_latest, 4);
}
```

- [ ] **Step 2: 실패 확인** — `cargo test -p loggen && cargo test -p iptracker-core --test e2e` → 컴파일 오류.
- [ ] **Step 3: 구현** (`rand`, `flate2`, `chrono`, CLI는 `clap` derive).
- [ ] **Step 4: 통과 확인** — `cargo test --workspace` → 전부 통과.
- [ ] **Step 5: 커밋** — `git add -A && git commit -m "가짜 access 로그 생성기와 통합 테스트 추가"`

---

### Task 7: Tauri 앱 껍데기와 명령

**Files:**
- Create: `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/icons/*`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/state.rs`, `src-tauri/src/config.rs`, `src-tauri/src/commands.rs`, `package.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `src/main.tsx`, `src/App.tsx`(빈 화면), `src/api.ts`
- Modify: `Cargo.toml`(멤버 추가)

**Interfaces:**
- Consumes: `iptracker_core`의 Task 1~5 인터페이스 전부.
- Produces — Tauri 명령(인자·반환은 camelCase JSON). `src/api.ts`에 같은 이름의 타입과 래퍼 함수를 둔다:

| 명령 | 인자 | 반환 |
|---|---|---|
| `scan_paths` | `paths: string[]` | `Candidate[]` (저장된 `serverRegex`를 적용) |
| `preview_file` | `path: string, logFormat?: string, timeFormat?: string` | `Preview` |
| `ingest_one` | `candidate: Candidate, conflict: "ask"\|"replace"\|"skip", allowHighFailure: boolean, deleteOriginal: boolean, logFormat?: string, timeFormat?: string` | `IngestResult` |
| `get_kpis` / `get_countries` / `get_daily` | `filter: Filter` | `Kpis` / `CountryRow[]` / `DayPoint[]` |
| `get_country_ips` | `filter, country, limit, offset` | `IpRow[]` |
| `get_ip_requests` | `period, ip, limit, offset` | `RequestRow[]` |
| `get_ingest_status` | – | `IngestStatus` |
| `get_settings` / `save_settings` | – / `settings: Settings` | `Settings` / `void` |
| `get_app_info` | – | `AppInfo` |
| `replace_geoip` | `path: string` | `AppInfo` |
| `purge_foreign` | `beforeDate: string` | `number` |
| `set_db_path` | `path: string` | `void`(다음 실행부터 적용) |

```ts
type FileSummary = { fileId: number; lines: number; failedLines: number; foreignRequests: number; dateFrom: string | null; dateTo: string | null };
type IngestOutcome =
  | ({ kind: "ingested" } & FileSummary)
  | { kind: "duplicate" } | { kind: "conflict" } | { kind: "skipped" }
  | { kind: "highFailure"; lines: number; failedLines: number; samples: string[] };
type IngestResult = { outcome: IngestOutcome; deleted: boolean; deleteError?: string };
type Settings = { serviceName: string; expectedServers: number; serverRegex: string; logFormat: string; timeFormat: string }; // 빈 문자열 = 미지정
type AppInfo = { version: string; geoBuildDate: string; geoIsBundled: boolean; dbPath: string; dbBytes: number };
```
- 이벤트 `ingest-progress`: `{ path: string, bytesRead: number, bytesTotal: number }`.

**정해 둔 동작:**
- 상태: `Mutex<rusqlite::Connection>`, `RwLock<iptracker_core::geo::GeoDb>`. 명령은 `tauri::async_runtime::spawn_blocking` 안에서 실행해 화면이 멈추지 않게 한다.
- 설정 기본값: `serviceName ""`, `expectedServers 16`, 나머지 `""`. `settings` 테이블에 저장한다. `save_settings`는 `serverRegex`가 정규식으로 컴파일되지 않거나 `logFormat`이 `LineParser::new`에서 실패하면 한국어 오류 문자열로 거부한다.
- DB 위치: 기본은 `app_local_data_dir`의 `ip-tracker.db`(Roaming 아님). `set_db_path`는 `db::copy_to`로 현재 DB를 새 경로에 복사한 뒤 앱 설정 폴더의 `config.json`(`{"dbPath": "..."}`)에 기록한다. 다음 실행부터 새 경로를 쓴다(`config.rs`). 복사에 실패하면 설정을 바꾸지 않는다.
- WAL: `ingest_one`과 `purge_foreign`이 끝날 때마다, 그리고 `RunEvent::Exit`에서 `db::compact`를 호출한다(종료가 `process::exit`라 연결이 정리되지 않아도 `-wal`이 남지 않게). `AppInfo.dbBytes`는 `.db`와 `-wal` 크기의 합.
- GeoIP: `app_local_data_dir`에 `geoip.mmdb`가 있으면 그것을, 없으면 내장 리소스 `resources/dbip-country-lite.mmdb`를 연다. `replace_geoip`는 고른 파일을 `GeoDb::open`으로 검증한 뒤 `app_local_data_dir`에 복사하고 메모리의 DB를 교체한다. 검증에 실패하면 기존 것을 유지하고 오류를 돌려준다.
- 파서 결정은 `preview_file`과 `ingest_one`이 **같은 함수** `resolve_parser(path, logFormat?, timeFormat?)`를 쓴다: 시간 형식 = 인자 `timeFormat` → 저장된 `timeFormat` → 없음. LogFormat = 인자 `logFormat` → 저장된 `logFormat` → `detect_file(path, 시간 형식)`. 화면(Task 9)은 미리보기에서 확정된 `Preview.logFormat`과 시간 형식을 `ingest_one`에 반드시 그대로 넘긴다(미리보기와 다른 형식으로 적재되는 일이 없게). 형식을 못 정하면 오류 `"로그 형식을 인식하지 못했습니다. 설정에서 LogFormat을 지정해 주세요."`.
- 원본 삭제: `deleteOriginal`이 true이고 결과가 `Ingested`일 때만 `std::fs::remove_file`. 삭제에 실패하면 적재는 그대로 두고 `deleted: false, deleteError: <메시지>`를 돌려준다. `Duplicate`·`Skipped`·`Conflict`·`HighFailure`는 지우지 않는다.
- `tauri.conf.json`: `productName "IP Tracker"`, `identifier "kr.iptracker.app"`, 창 1400×900, `security.csp` = `"default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'"`, `bundle.resources`에 `resources/*`, `bundle.targets ["nsis"]`, `bundle.windows.webviewInstallMode.type "skip"`.
- 플러그인: `tauri-plugin-dialog`(파일·폴더 선택)만 쓴다. 끌어다 놓기는 Tauri 창의 drag-drop 이벤트로 경로를 받는다.

- [ ] **Step 1: 실패하는 테스트 작성** (`src-tauri/src/commands.rs`의 테스트 모듈. 명령 본체를 `tauri::State` 없이 호출할 수 있는 일반 함수로 분리해 테스트한다.)

```rust
#[test] fn ingest_deletes_original_only_when_ingested() {
    // 임시 폴더에 정상 로그 1개. do_ingest(..., delete_original=true) → outcome Ingested, deleted=true, 파일 없음
    // 같은 내용을 다시 만들어 넣으면 → outcome Duplicate, deleted=false, 파일 남아 있음
}
#[test] fn delete_failure_keeps_data_and_reports() {
    // 삭제 함수를 주입 가능하게 하고(fn(&Path) -> io::Result<()>), 항상 PermissionDenied를 돌려주는 것으로 호출
    // → outcome Ingested, deleted=false, delete_error.is_some(), ip_daily에 데이터 있음
}
#[test] fn preview_and_ingest_resolve_the_same_parser() {
    // 저장된 logFormat = COMMON, 인자 logFormat = COMBINED → resolve_parser 결과가 COMBINED
    // 인자 없음 → COMMON. 인자 timeFormat만 "%d.%m.%Y %H:%M:%S" → detect_file이 그 시간 형식으로 인식
}
#[test] fn scan_applies_saved_server_regex() { /* serverRegex "^(was\\d+)_" 저장 후 logs/was07_a.log → server "was07" */ }
#[test] fn unrecognized_format_is_a_korean_error() {
    assert_eq!(do_ingest(garbage_file, ...).unwrap_err(), "로그 형식을 인식하지 못했습니다. 설정에서 LogFormat을 지정해 주세요.");
}
#[test] fn settings_roundtrip_and_validation() {
    // 기본값 확인(expected_servers == 16) → 저장 → 다시 읽으면 같음
    // server_regex "(" → Err, log_format "%t %>s" → Err
}
#[test] fn replace_geoip_rejects_invalid_file() { /* 텍스트 파일을 넘기면 Err, 기존 GeoDb로 8.8.8.8 → US 여전히 동작 */ }
```

- [ ] **Step 2: 실패 확인** — `cargo test -p ip-tracker` → 컴파일 오류.
- [ ] **Step 3: 구현** — `npm create tauri-app` 템플릿을 참고하되 파일은 위 목록대로 직접 만든다. `src/App.tsx`는 `get_app_info` 결과의 버전만 보여주는 임시 화면.
- [ ] **Step 4: 통과 확인** — `cargo test --workspace` 통과, `npm install && npm run build` 성공, `npm run tauri dev`로 창이 뜨고 버전이 보인다.
- [ ] **Step 5: 커밋** — `git add -A && git commit -m "Tauri 앱 껍데기와 명령 추가"`

---

### Task 8: 화면 기반과 대시보드

**Files:**
- Create: `src/lib/period.ts`, `src/lib/format.ts`, `src/lib/countryName.ts`, `src/lib/*.test.ts`, `src/screens/Dashboard.tsx`, `src/components/{KpiTiles,WorldMap,TopCountries,DailyTrend,CountryTable,IpDrawer,Notices}.tsx`, `src/assets/world.geo.json`, `src/styles.css`
- Modify: `src/App.tsx`, `package.json`(`echarts`, `vitest`)

**Interfaces:**
- Consumes: `src/api.ts`(Task 7).
- Produces:
```ts
// period.ts
export type Preset = "latest" | "7d" | "30d" | "custom";
export function presetPeriod(preset: Exclude<Preset, "custom">, latestDate: string | null, today: string): Period; // latestDate가 null이면 today 기준
export function periodDays(p: Period): number;
// format.ts
export function num(n: number): string;                    // 4812330 → "4,812,330"
export function pct(ratio: number, digits?: number): string; // 0.0204 → "2.0%", NaN/Infinity → "0.0%"
export function delta(cur: number, prev: number): string;  // "+3.1%", "-2.0%", prev가 0이면 "–"
export function dateTime(ts: number): string;              // 유닉스 초 → "11/22 23:58" (KST)
// countryName.ts
export function countryName(iso2: string): string;         // "US" → "미국", 모르는 코드는 코드 그대로
// 차트 컴포넌트 — 보고서(Task 10)가 같은 컴포넌트를 재사용한다
type ChartCommon = { height: number | string; renderer?: "canvas" | "svg"; animation?: boolean }; // 기본 canvas, true
export function WorldMap(p: { rows: CountryRow[] } & ChartCommon): JSX.Element;
export function TopCountries(p: { rows: CountryRow[]; top: number } & ChartCommon): JSX.Element; // top개 + "기타 n개국"
export function DailyTrend(p: { points: DayPoint[]; series: ("requests" | "ips")[] } & ChartCommon): JSX.Element;
```
- `App.tsx`가 `filter: { preset, period, exclude404 }` 상태를 들고 `Dashboard`와 `Report`(Task 10)에 넘긴다. 상단 탭: `대시보드` / `로그 적재` / `설정`.

**정해 둔 동작과 문구:**
- 구현 전에 두 목업 파일을 읽는다. 항목·순서는 설계 문서 6.4와 목업을 따른다: 상단 줄 → 제외 스위치 → 핵심 숫자 5칸 → 세계지도 + 상위 10개국 막대 → 일별 추이 → 국가별 상세 표 → 안내사항.
- 기간 버튼: `최근 적재일` / `최근 7일` / `최근 30일` / `기간 지정`. 기준일은 `get_ingest_status().latestDate`(목업의 "어제" 자리. 수동 적재라 어제 데이터가 없을 수 있어 최근 적재일을 기준으로 한다). 기본 선택은 `최근 7일`.
- 적재 현황 문구: `적재 현황: {M/D}까지 · 서버 {serversOnLatest}/{expectedServers}`. 데이터가 없으면 `적재된 로그가 없습니다`.
- 스위치 문구: `404를 낸 IP 제외`, 옆 설명 `켜면 아래 모든 숫자·지도·보고서에서 빠집니다`.
- 핵심 숫자 5칸(제목 / 보조 문구): `전체 요청` / 직전 기간 대비 증감, `해외 요청` / `전체의 {pct}`, `해외 고유 IP` / `국내 고유 IP {n}`, `그중 404를 낸 IP`(`foreignIps404`) / `해외 IP의 {pct}`(분모는 스위치를 끈 상태의 해외 고유 IP = `cur.foreignIps + (exclude404 ? foreignIps404 : 0)`), `접속 국가 수` / `신규 국가 {n}`.
- `unknownRequests`가 0보다 크면 핵심 숫자 아래에 `국가를 판별하지 못한 요청 {n}건은 집계에서 제외했습니다.`를 작게 표시한다.
- 세계지도: `src/assets/world.geo.json`은 Natural Earth 110m admin-0 countries GeoJSON에서 속성을 `ISO_A2_EH`만 남겨 줄인 것. `echarts.registerMap("world", geo)` 후 series에 `nameProperty: "ISO_A2_EH"`를 주어 국가 코드로 연결한다. 한국은 데이터에서 뺀다. 색은 한 가지 색의 농도(요청 수), 툴팁은 `{국가명}<br>요청 {n}건 · IP {n}개`.
- 상위 10개국 막대: 10개국 + 나머지를 합친 `기타 {n}개국`(회색).
- 일별 추이: 선 2개 `해외 요청 수`, `해외 고유 IP 수`. 값 차이가 크므로 y축을 좌·우로 나눈다.
- 국가별 상세 표의 열: `국가` / `요청 수` / `해외 중 비율` / `고유 IP` / `404 요청 수` / `404를 낸 고유 IP` / `최근 접속`. 열 제목을 누르면 정렬. 행을 누르면 `IpDrawer`가 열려 그 국가의 IP 목록(`IP` / `요청 수` / `404 수` / `처음 접속` / `마지막 접속`, 50개씩 더 보기)을 보여주고, IP를 누르면 그 IP의 개별 요청(`시간` / `메서드` / `URL` / `상태` / `서버` / `User-Agent`, 100개씩 더 보기)을 보여준다.
- 안내사항(`Notices`, 화면 맨 아래, 문구 그대로):
  - `국가는 접속 IP 주소 기준으로 판별하며, VPN 등으로 실제 위치와 다를 수 있습니다.`
  - `"404를 낸 IP 제외"를 켜면, 없어진 주소를 호출하는 구버전 앱 이용자도 함께 빠질 수 있습니다.`
  - `통신사·공용 와이파이처럼 여러 사람이 한 IP를 쓰는 경우, 한 명의 404로 같은 IP의 다른 이용자도 함께 빠질 수 있습니다.`
- 국가명은 `new Intl.DisplayNames(["ko"], { type: "region" })`로 얻는다(별도 데이터 파일 없음).
- 색: 주색 `#1f4e8c`, 보조 `#c9792b`, 회색 `#9aa7b6`, 테두리 `#dde3ea`(목업과 동일).
- 데이터가 없는 기간에는 지도·막대·표 자리에 `해당 기간에 데이터가 없습니다`를 보여주고, 어떤 숫자도 `NaN`·`Infinity`로 나오지 않는다.

- [ ] **Step 1: 실패하는 테스트 작성** (`src/lib/period.test.ts`, `format.test.ts`, `countryName.test.ts`)

```ts
test("presets anchor on latest ingested date", () => {
  expect(presetPeriod("latest", "2026-11-22", "2026-11-23")).toEqual({ from: "2026-11-22", to: "2026-11-22" });
  expect(presetPeriod("7d", "2026-11-22", "2026-11-23")).toEqual({ from: "2026-11-16", to: "2026-11-22" });
  expect(presetPeriod("30d", "2026-11-22", "2026-11-23")).toEqual({ from: "2026-10-24", to: "2026-11-22" });
  expect(presetPeriod("7d", null, "2026-11-23")).toEqual({ from: "2026-11-17", to: "2026-11-23" });
  expect(periodDays({ from: "2026-11-16", to: "2026-11-22" })).toBe(7);
});
test("number formatting never shows NaN", () => {
  expect(num(4812330)).toBe("4,812,330");
  expect(pct(0.0204)).toBe("2.0%");
  expect(pct(0 / 0)).toBe("0.0%");
  expect(pct(1 / 0)).toBe("0.0%");
  expect(delta(103.1, 100)).toBe("+3.1%");
  expect(delta(98, 100)).toBe("-2.0%");
  expect(delta(5, 0)).toBe("–");
  expect(dateTime(1795359480)).toBe("11/22 23:58"); // 2026-11-22T23:58:00+09:00
});
test("country names are Korean", () => {
  expect(countryName("US")).toBe("미국");
  expect(countryName("JP")).toBe("일본");
  expect(countryName("XX")).toBe("XX");
});
```

- [ ] **Step 2: 실패 확인** — `npx vitest run` → 모듈 없음으로 실패.
- [ ] **Step 3: lib 구현 후 통과 확인** — `npx vitest run` → 통과.
- [ ] **Step 4: 대시보드 화면 구현** — 위 "정해 둔 동작과 문구"대로.
- [ ] **Step 5: 실제 화면 확인** — 적재 화면이 아직 없으므로 이 단계에서는 빈 DB 상태만 `npm run tauri dev`로 확인한다(`NaN` 없음, `적재된 로그가 없습니다` 표시). 데이터가 있는 상태의 확인은 Task 9의 Step 5에서 함께 한다. 그때 볼 항목: 목업 `01-dashboard-full.html`과 항목·순서가 같은지, 스위치를 켜면 모든 숫자가 바뀌는지, 빈 DB에서 `NaN`이 없는지, 창 너비 1200px에서 가로 스크롤이 없는지.
- [ ] **Step 6: 커밋** — `git add -A && git commit -m "대시보드 화면 추가"`

---

### Task 9: 로그 적재 화면

**Files:**
- Create: `src/screens/Ingest.tsx`, `src/lib/ingestQueue.ts`, `src/lib/ingestQueue.test.ts`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `scan_paths`, `preview_file`, `ingest_one`, 이벤트 `ingest-progress`, `get_settings`(Task 7).
- Produces:
```ts
// ingestQueue.ts — 화면과 분리한 적재 순서 로직(테스트 대상)
export type Decision = "replace" | "skip";
export type RowState = "대기" | "적재 중" | "완료" | "중복(건너뜀)" | "건너뜀" | "실패";
export type Row = { candidate: Candidate; state: RowState; summary?: FileSummary; message?: string; deleted?: boolean };
export async function runQueue(
  candidates: Candidate[],
  deps: {
    ingestOne(c: Candidate, conflict: "ask" | "replace" | "skip", allowHighFailure: boolean): Promise<IngestResult>;
    askConflict(c: Candidate): Promise<{ decision: Decision; applyToAll: boolean }>;
    askHighFailure(c: Candidate, lines: number, failed: number, samples: string[]): Promise<boolean>;
    onRow(index: number, row: Row): void;
  }
): Promise<Row[]>;
```

**정해 둔 동작과 문구:**
- `ingest_one`에는 미리보기에서 확정된 LogFormat과 시간 형식을 항상 넘긴다.
- 흐름: ① 파일·폴더를 끌어다 놓거나 `파일 선택`/`폴더 선택` → ② `scan_paths`로 목록(서버 이름 / 파일명 / 크기) 표시. 서버 이름은 칸에서 직접 고칠 수 있다 → ③ 목록 첫 파일의 미리보기 표(`IP` / `시간` / `메서드` / `URL` / `상태코드`, 20줄)와 인식된 형식 표시. 다른 파일을 누르면 그 파일의 미리보기로 바뀐다 → ④ `적재 시작`을 눌러야 적재한다.
- 미리보기에서 형식을 인식하지 못하면 `로그 형식을 자동으로 인식하지 못했습니다. 아파치 LogFormat을 붙여 넣어 주세요.`와 입력 칸(LogFormat, 시간 형식)을 보여주고 `다시 미리보기` 버튼을 둔다. 인식됐더라도 `형식 직접 지정`을 눌러 같은 입력 칸을 열 수 있다. 여기서 지정한 형식은 `이 형식을 저장` 체크 시 `save_settings`로 저장한다.
- 형식을 인식하지 못한 상태에서는 `적재 시작`을 누를 수 없다.
- 체크박스 `적재가 끝나면 원본 파일 삭제`(기본 꺼짐, 마지막 선택을 `localStorage`에 기억). 옆 설명 `DB에 저장이 끝난 파일만 지웁니다. 휴지통을 거치지 않습니다.`
- `runQueue`는 파일을 하나씩 순서대로 처리한다. 결과별 처리:
  - `conflict` → `askConflict`. 문구: `{서버} / {파일명}은 이미 적재된 파일과 내용이 다릅니다. 기존 것을 대체할까요?` 버튼 `대체` / `건너뛰기`, 체크 `남은 파일에도 똑같이 적용`. 답에 따라 같은 파일을 `replace` 또는 `skip`으로 다시 호출한다.
  - `highFailure` → `askHighFailure`. 문구: `{파일명}에서 {lines}줄 중 {failed}줄을 읽지 못했습니다. 형식이 맞는지 확인해 주세요.` 아래 실패 줄 예시, 버튼 `그래도 적재` / `이 파일 건너뛰기`. `그래도 적재`면 `allowHighFailure=true`로 다시 호출한다. 이때 conflict 인자는 이미 정한 값(`replace`/`skip`)을 유지하고 `ask`로 되돌리지 않는다.
  - `ingestOne`이 오류를 던지면 그 행을 `실패`(오류 메시지 표시)로 두고 다음 파일로 넘어간다.
- 진행: 파일별 진행 막대(`ingest-progress`)와 전체 `{끝난 수}/{전체 수}`. 끝나면 요약 `적재 {n}개 · 중복 {n}개 · 건너뜀 {n}개 · 실패 {n}개, 해외 요청 {n}건`.
- 원본 삭제에 실패한 행에는 `적재는 완료했지만 원본을 지우지 못했습니다: {오류}`를 표시한다.
- 적재가 끝나면 대시보드가 다음에 열릴 때 새 데이터를 읽도록 `App`의 새로고침 키를 올린다.

- [ ] **Step 1: 실패하는 테스트 작성** (`ingestQueue.test.ts`, `deps`는 가짜 함수)

```ts
test("conflict asks once, then retries with the answer", async () => {
  // ingestOne: 첫 호출(ask) → conflict, 둘째 호출(replace) → ingested
  // askConflict → { decision: "replace", applyToAll: false }
  expect(calls).toEqual([["a", "ask", false], ["a", "replace", false]]);
  expect(rows[0].state).toBe("완료");
});
test("apply-to-all skips further prompts", async () => {
  // 파일 2개 모두 conflict, 첫 답이 { decision: "skip", applyToAll: true }
  expect(askConflict).toHaveBeenCalledTimes(1);
  expect(rows.map(r => r.state)).toEqual(["건너뜀", "건너뜀"]);
  // 둘째 파일은 처음부터 "skip"으로 호출된다
});
test("high failure retries only when confirmed", async () => {
  // 파일1: highFailure → 확인 true → allowHighFailure=true로 재호출 → ingested
  // 파일2: highFailure → 확인 false → 재호출 없음, state "건너뜀"
});
test("high failure after replace keeps the replace decision", async () => {
  // ingestOne: (ask,false) → conflict, (replace,false) → highFailure, (replace,true) → ingested
  expect(calls).toEqual([["a", "ask", false], ["a", "replace", false], ["a", "replace", true]]);
  expect(askConflict).toHaveBeenCalledTimes(1);
});
test("a thrown error marks the row failed and continues", async () => {
  expect(rows.map(r => r.state)).toEqual(["실패", "완료"]);
  expect(rows[0].message).toBe("읽기 오류");
});
test("duplicate is reported without prompting", async () => {
  expect(rows[0].state).toBe("중복(건너뜀)");
});
test("delete failure is carried on the row", async () => {
  // ingestOne → { outcome: ingested, deleted: false, deleteError: "사용 중" }
  expect(rows[0]).toMatchObject({ state: "완료", deleted: false, message: "적재는 완료했지만 원본을 지우지 못했습니다: 사용 중" });
});
```

- [ ] **Step 2: 실패 확인** — `npx vitest run src/lib/ingestQueue.test.ts` → 실패.
- [ ] **Step 3: `runQueue` 구현 후 통과 확인.**
- [ ] **Step 4: 적재 화면 구현.**
- [ ] **Step 5: 실제 화면 확인(Task 8의 Step 5 항목 포함)** — `cargo run -p loggen -- --out /tmp/iptracker-logs --servers 16 --days 7 --end 2026-11-22 --gzip`로 만든 폴더를 끌어다 놓아 미리보기 → 적재 → 대시보드 확인. 같은 폴더를 다시 넣으면 전부 `중복(건너뜀)`인지, 파일 하나의 내용을 바꿔 다시 넣으면 대체 여부를 묻는지, 원본 삭제를 켜면 적재된 파일만 사라지는지 확인한다. 설정 화면의 DB 크기로 "서버 16대 × 하루" 적재당 DB 증가량을 기록해 요청자에게 알린다(1년치로 환산해 수 GB를 넘으면 `ip_daily` 압축을 후속 작업으로 제안). 화면을 캡처해 목업과 나란히 놓고 요청자에게 보여준다.
- [ ] **Step 6: 커밋** — `git add -A && git commit -m "로그 적재 화면 추가"`

---

### Task 10: 보고서 내보내기

**Files:**
- Create: `src/screens/Report.tsx`, `src/lib/summary.ts`, `src/lib/summary.test.ts`, `src/report.css`
- Modify: `src/App.tsx`, `src/screens/Dashboard.tsx`(`보고서 내보내기` 버튼)

**Interfaces:**
- Consumes: `get_kpis`, `get_countries`, `get_daily`, `get_settings`, `get_app_info`, `App`의 `filter` 상태, `WorldMap`·`TopCountries`·`DailyTrend` 컴포넌트와 props(Task 8).
- Produces:
```ts
export function summarySentence(a: { days: number; countries: number; foreignIps: number; foreignShare: number; top: { name: string; share: number }[] }): string;
export function reportTitle(serviceName: string): string; // "" → "해외 접속 현황", "민원24" → "민원24 해외 접속 현황"
export function reportFootnotes(a: { exclude404: boolean; excludedIps: number; geoBuildDate: string }): string[];
```

**정해 둔 동작과 문구:**
- 구현 전에 목업 `02-404-and-report-final.html`의 ② 보고서를 읽고 그 구성을 따른다. A4 세로 한 장: 제목 줄(제목 + 오른쪽에 `2026. 11. 16. ~ 11. 22. (7일)`) → 요약 문단 → 숫자 카드 4개 → 세계지도 + 상위 5개국 막대 → 일별 추이 → 하단 안내.
- 숫자 카드(제목 / 값 / 보조): `해외 요청` / 건수 / `직전 기간 대비 {delta}`, `해외 접속 IP` / 개수 / `직전 기간 대비 {delta}`, `접속 국가` / 개수 / `신규 {n}개국`, `해외 비중` / 비율 / `직전 기간 {pct}`.
- 요약 문단: `지난 {days}일간 {countries}개국 {foreignIps}개 IP가 해외에서 접속했습니다. 전체 요청의 {foreignShare}이며, 상위 {k}개국({이름·이름·이름})이 해외 요청의 {합계}를 차지합니다.` `k`는 `top`의 개수(최대 3). 해외 접속이 없으면 `지난 {days}일간 해외 접속이 없었습니다.`
- 하단 안내(순서대로): `국가는 접속 IP 주소 기준으로 판별했으며, VPN 등으로 실제 위치와 다를 수 있습니다.` / (스위치가 켜졌을 때만) `404 응답을 받은 IP {n}개(공격 시도 추정)는 집계에서 제외했습니다.` / `IP 위치 정보: DB-IP (db-ip.com), CC BY 4.0, {geoBuildDate} 기준`.
- 대시보드의 현재 기간과 스위치 상태를 그대로 쓴다.
- 보고서 화면은 미리보기와 `인쇄 / PDF 저장` 버튼, `돌아가기` 버튼으로 구성한다. 버튼은 `window.print()`를 호출한다(Windows 인쇄 창에서 "Microsoft Print to PDF"를 고르면 PDF로 저장된다. 버튼 옆에 이 안내를 한 줄로 적는다).
- 인쇄 CSS: `@page { size: A4 portrait; margin: 0; }`(여백이 0이면 WebView2가 날짜·주소·쪽번호 머리글·바닥글을 찍지 않는다). 보고서 루트는 `width: 210mm; min-height: 297mm; padding: 14mm; box-sizing: border-box`. `@media print`에서 버튼·탭을 숨기고 보고서만 한 장에 들어가게 한다. 흑백 인쇄에서도 구분되게 지도와 막대는 한 가지 색의 농도만 쓴다.
- 차트는 `renderer: "svg"`, `animation: false`로, mm 단위로 크기를 고정한 컨테이너 안에 그린다(화면 너비에 따라 넘치거나 잘리지 않게).

- [ ] **Step 1: 실패하는 테스트 작성**

```ts
test("summary sentence with three top countries", () => {
  expect(summarySentence({ days: 7, countries: 41, foreignIps: 3154, foreignShare: 0.02,
    top: [{ name: "미국", share: 0.33 }, { name: "일본", share: 0.2 }, { name: "중국", share: 0.13 }] }))
    .toBe("지난 7일간 41개국 3,154개 IP가 해외에서 접속했습니다. 전체 요청의 2.0%이며, 상위 3개국(미국·일본·중국)이 해외 요청의 66.0%를 차지합니다.");
});
test("summary sentence with one country", () => {
  expect(summarySentence({ days: 1, countries: 1, foreignIps: 2, foreignShare: 0.5, top: [{ name: "미국", share: 1 }] }))
    .toBe("지난 1일간 1개국 2개 IP가 해외에서 접속했습니다. 전체 요청의 50.0%이며, 상위 1개국(미국)이 해외 요청의 100.0%를 차지합니다.");
});
test("summary sentence with no foreign access", () => {
  expect(summarySentence({ days: 7, countries: 0, foreignIps: 0, foreignShare: 0, top: [] })).toBe("지난 7일간 해외 접속이 없었습니다.");
});
test("title and footnotes", () => {
  expect(reportTitle("")).toBe("해외 접속 현황");
  expect(reportTitle("민원24")).toBe("민원24 해외 접속 현황");
  expect(reportFootnotes({ exclude404: false, excludedIps: 318, geoBuildDate: "2026-11-01" })).toHaveLength(2);
  expect(reportFootnotes({ exclude404: true, excludedIps: 318, geoBuildDate: "2026-11-01" })[1])
    .toBe("404 응답을 받은 IP 318개(공격 시도 추정)는 집계에서 제외했습니다.");
});
```

- [ ] **Step 2: 실패 확인** — `npx vitest run src/lib/summary.test.ts` → 실패.
- [ ] **Step 3: `summary.ts` 구현 후 통과 확인.**
- [ ] **Step 4: 보고서 화면과 인쇄 CSS 구현.**
- [ ] **Step 5: 실제 확인** — `npm run tauri dev`에서 보고서를 열어 PDF로 저장하고, 한 장에 들어가는지와 목업과 구성이 같은지 확인한다. PDF를 요청자에게 보여준다. (Mac의 웹뷰는 Windows WebView2와 인쇄 결과가 다르므로, 최종 확인은 Task 12의 Windows 확인에서 한다.)
- [ ] **Step 6: 커밋** — `git add -A && git commit -m "보고서 내보내기 추가"`

---

### Task 11: 설정 화면

**Files:**
- Create: `src/screens/Settings.tsx`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `get_settings`, `save_settings`, `get_app_info`, `replace_geoip`, `purge_foreign`, `set_db_path`(Task 7), `num`(Task 8).

**정해 둔 동작과 문구:**
- 구역과 항목:
  - **기본**: `서비스 이름`(보고서 제목에 사용), `서버 대수`(숫자, 기본 16).
  - **로그 형식**: `아파치 LogFormat`(비우면 자동 인식), `시간 형식`(비우면 자동 인식, 예시 `%d/%b/%Y:%H:%M:%S %z`), `서버 이름 추출 규칙`(정규식, 첫 번째 괄호가 서버 이름. 비우면 폴더 이름 사용. 예시 `^(was\d+)_`).
  - **IP 위치 DB**: `현재 DB 기준일 {geoBuildDate}`와 `내장 DB` 또는 `교체한 DB` 표시, `새 DB 파일 불러오기(.mmdb)` 버튼. 설명 `교체해도 이미 적재한 데이터의 국가는 바뀌지 않습니다.`
  - **저장 공간**: `DB 위치 {dbPath}`, `DB 크기 {MB}`, `DB 위치 변경` 버튼(설명 `현재 DB를 새 위치로 복사하고, 다음 실행부터 새 위치를 씁니다. 기존 파일은 확인 후 직접 지워 주세요.`), `오래된 해외 요청 상세 삭제`(개월 수 입력, 기본 6, 버튼 `삭제`). 삭제 전 확인 문구 `{날짜} 이전의 해외 요청 상세를 지웁니다. 국가별 통계는 그대로 남습니다. 계속할까요?`, 완료 문구 `{n}건을 지웠습니다.`
  - **정보**: 앱 버전, `IP 위치 정보: DB-IP (db-ip.com), CC BY 4.0`.
- `저장`을 누르면 `save_settings`를 호출하고, 거부되면 돌려받은 오류 문구를 해당 구역 아래에 보여준다.
- 삭제 기준일은 오늘(KST)에서 입력한 개월 수를 뺀 날짜다.

- [ ] **Step 1: 설정 화면 구현.**
- [ ] **Step 2: 실제 확인** — 서비스 이름을 저장하면 보고서 제목에 반영되는지, 잘못된 정규식 `(`를 저장하면 오류가 보이는지, 텍스트 파일을 IP 위치 DB로 불러오면 거부되는지, 상세 삭제 후 대시보드의 국가별 숫자가 그대로인지 확인한다.
- [ ] **Step 3: 커밋** — `git add -A && git commit -m "설정 화면 추가"`

---

### Task 12: 오프라인 검사와 Windows 릴리즈

**Files:**
- Create: `scripts/check-offline.mjs`, `.github/workflows/release.yml`, `README.md`
- Modify: `package.json`(스크립트 `check:offline`, `test`)

**정해 둔 동작:**
- `scripts/check-offline.mjs`: `dist/` 아래 모든 `.html`·`.js`·`.css`에서 `https?://` 주소를 찾는다. 허용 목록(`http://www.w3.org/`로 시작하는 XML 네임스페이스, `https://db-ip.com` 출처 표기 문자열)을 뺀 나머지가 하나라도 있으면 파일명·주소를 출력하고 종료 코드 1. 없으면 `외부 주소 없음`을 출력하고 0. ECharts 등 라이브러리 안에 문서 링크 문자열이 걸리면, 실제 요청이 아닌 문자열임을 확인한 뒤 정확한 주소 단위로 허용 목록에 추가하고 이유를 주석으로 남긴다.
- `package.json`: `"test": "vitest run"`, `"check:offline": "node scripts/check-offline.mjs"`.
- `release.yml`: `v*` 태그 푸시에 실행, `windows-latest`, Node 22와 Rust stable 설치 → `npm ci` → `npm test` → `cargo test --workspace` → `npm run build` → `npm run check:offline` → `tauri-apps/tauri-action@v0`로 NSIS 설치 파일을 만들어 해당 태그의 GitHub 릴리즈(초안)에 올린다.
- `README.md`(한국어): 무엇을 하는 도구인지, 개발 실행(`npm install`, `npm run tauri dev`), 가짜 로그 만들기(loggen 명령), 릴리즈 만드는 법(태그 푸시), 폐쇄망 반입 후 첫 실행 때 확인할 것(미리보기에서 IP·시간이 맞는지, 안 맞으면 설정에서 LogFormat 지정. 하루 파일의 첫 줄과 끝 줄 시간이 KST 0시 무렵~24시 무렵인지 — rotatelogs가 UTC 기준으로 파일을 나누면 한 파일이 이틀에 걸쳐 최근 날짜가 9시간치만 보일 수 있음), GeoIP 출처 표기.

- [ ] **Step 1: 검사 스크립트의 실패 확인** — `dist/`에 `https://example.com/x.js`를 참조하는 임시 파일을 두고 `npm run check:offline` → 종료 코드 1과 해당 주소 출력. 임시 파일 삭제.
- [ ] **Step 2: 통과 확인** — `npm run build && npm run check:offline` → `외부 주소 없음`.
- [ ] **Step 3: 전체 검사** — `cargo test --workspace && npm test` → 전부 통과.
- [ ] **Step 4: 네트워크 차단 실행 확인** — Mac의 Wi-Fi를 끈 상태로 `npm run tauri build` 결과물을 실행해 적재·대시보드·지도·보고서가 모두 동작하는지 확인한다(요청자에게 Wi-Fi 끄기를 부탁하거나, 개발자 도구의 네트워크 탭에 외부 요청이 0건인지로 대신한다).
- [ ] **Step 5: 커밋** — `git add -A && git commit -m "오프라인 검사와 Windows 릴리즈 워크플로 추가"`
- [ ] **Step 6: 릴리즈는 요청자 확인 후** — GitHub 원격 저장소 생성·푸시·태그는 요청자가 지시할 때만 한다.
- [ ] **Step 7: Windows 확인(요청자와 함께)** — 릴리즈 설치 파일을 Windows PC에 설치해 loggen 로그로 적재하고, 보고서를 "Microsoft Print to PDF"로 저장해 머리글·바닥글이 없고 A4 한 장에 들어가는지 확인한다. 폐쇄망 반입 전에 한다.
