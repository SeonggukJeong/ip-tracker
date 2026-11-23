//! Tauri 명령. 본체는 tauri::State 없이 부를 수 있는 일반 함수(do_*)로 두고,
//! #[tauri::command] 함수는 그것을 spawn_blocking 안에서 부르기만 한다. 오류는 한국어 문자열로 넘긴다.

use std::fmt::Display;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Mutex, RwLock};

use iptracker_core::geo::GeoDb;
use iptracker_core::ingest::{self, Candidate, ConflictPolicy, IngestOptions, IngestOutcome, Preview};
use iptracker_core::logformat::LineParser;
use iptracker_core::query::{self, CountryRow, DayPoint, Filter, IngestStatus, IpRow, Kpis, Period, RequestRow};
use iptracker_core::db;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::{self, Config};
use crate::state::{lock, AppState, LOCAL_GEOIP_FILE};

const UNRECOGNIZED: &str = "로그 형식을 인식하지 못했습니다. 설정에서 LogFormat을 지정해 주세요.";
const DEFAULT_EXPECTED_SERVERS: u32 = 16;

const KEY_SERVICE_NAME: &str = "service_name";
const KEY_EXPECTED_SERVERS: &str = "expected_servers";
const KEY_SERVER_REGEX: &str = "server_regex";
const KEY_LOG_FORMAT: &str = "log_format";
const KEY_TIME_FORMAT: &str = "time_format";

/// 문자열 값은 빈 문자열이 "미지정".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub service_name: String,
    pub expected_servers: u32,
    pub server_regex: String,
    pub log_format: String,
    pub time_format: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestResult {
    pub outcome: IngestOutcome,
    pub deleted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    /// "YYYY-MM-DD"
    pub geo_build_date: String,
    pub geo_is_bundled: bool,
    pub db_path: String,
    /// .db와 -wal 크기의 합
    pub db_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IngestProgress {
    path: String,
    bytes_read: u64,
    bytes_total: u64,
}

fn err(e: impl Display) -> String {
    e.to_string()
}

/// None과 빈 문자열을 같은 "미지정"으로 본다.
fn given(s: Option<&str>) -> Option<&str> {
    s.filter(|s| !s.trim().is_empty())
}

pub fn do_get_settings(conn: &Connection) -> Result<Settings, String> {
    let get = |key| db::get_setting(conn, key).map(Option::unwrap_or_default).map_err(err);
    let expected_servers = get(KEY_EXPECTED_SERVERS)?.parse().unwrap_or(DEFAULT_EXPECTED_SERVERS);
    Ok(Settings {
        service_name: get(KEY_SERVICE_NAME)?,
        expected_servers,
        server_regex: get(KEY_SERVER_REGEX)?,
        log_format: get(KEY_LOG_FORMAT)?,
        time_format: get(KEY_TIME_FORMAT)?,
    })
}

pub fn do_save_settings(conn: &Connection, s: &Settings) -> Result<(), String> {
    if let Some(re) = given(Some(&s.server_regex)) {
        regex::Regex::new(re).map_err(|e| format!("서버 이름 정규식이 올바르지 않습니다: {e}"))?;
    }
    if let Some(f) = given(Some(&s.log_format)) {
        LineParser::new(f, given(Some(&s.time_format))).map_err(|e| format!("LogFormat이 올바르지 않습니다: {e}"))?;
    }
    let tx = conn.unchecked_transaction().map_err(err)?;
    for (key, value) in [
        (KEY_SERVICE_NAME, s.service_name.as_str()),
        (KEY_EXPECTED_SERVERS, &s.expected_servers.to_string()),
        (KEY_SERVER_REGEX, &s.server_regex),
        (KEY_LOG_FORMAT, &s.log_format),
        (KEY_TIME_FORMAT, &s.time_format),
    ] {
        db::set_setting(&tx, key, value).map_err(err)?;
    }
    tx.commit().map_err(err)
}

pub fn do_scan(conn: &Connection, paths: &[PathBuf]) -> Result<Vec<Candidate>, String> {
    let s = do_get_settings(conn)?;
    let re = match given(Some(&s.server_regex)) {
        Some(re) => Some(regex::Regex::new(re).map_err(|e| format!("서버 이름 정규식이 올바르지 않습니다: {e}"))?),
        None => None,
    };
    Ok(ingest::scan(paths, re.as_ref()))
}

/// 미리보기와 적재가 함께 쓰는 파서 결정.
/// 시간 형식 = 인자 → 저장값 → 없음. LogFormat = 인자 → 저장값 → 파일 앞부분으로 자동 인식.
/// 자동 인식에 실패하면 Ok(None).
pub fn resolve_parser(
    conn: &Connection,
    path: &Path,
    log_format: Option<&str>,
    time_format: Option<&str>,
) -> Result<Option<LineParser>, String> {
    let saved = do_get_settings(conn)?;
    let time_format = given(time_format).or(given(Some(&saved.time_format)));
    match given(log_format).or(given(Some(&saved.log_format))) {
        Some(f) => LineParser::new(f, time_format).map(Some).map_err(|e| format!("LogFormat이 올바르지 않습니다: {e}")),
        None => ingest::detect_file(path, time_format).map_err(err),
    }
}

/// 형식을 정하지 못하면 오류 대신 log_format=None인 미리보기를 돌려준다(화면이 직접 지정 칸을 띄운다).
pub fn do_preview(
    conn: &Connection,
    path: &Path,
    log_format: Option<&str>,
    time_format: Option<&str>,
) -> Result<Preview, String> {
    match resolve_parser(conn, path, log_format, time_format)? {
        Some(parser) => ingest::preview(path, Some(&parser)).map_err(err),
        None => {
            // 표본 줄 수만 쓴다. 형식은 위의 결정을 따르므로 여기서 다시 고르지 않는다.
            let sampled = ingest::preview(path, None).map_err(err)?.sampled;
            Ok(Preview { log_format: None, rows: Vec::new(), sampled, failed: sampled })
        }
    }
}

/// 적재 후 delete_original이고 결과가 Ingested일 때만 delete(원본)를 부른다.
/// 삭제에 실패해도 적재는 그대로 두고 delete_error로 알린다.
#[allow(clippy::too_many_arguments)]
pub fn do_ingest(
    conn: &Mutex<Connection>,
    geo: &RwLock<GeoDb>,
    candidate: &Candidate,
    opts: &IngestOptions,
    delete_original: bool,
    log_format: Option<&str>,
    time_format: Option<&str>,
    progress: &mut dyn FnMut(u64, u64),
    delete: fn(&Path) -> io::Result<()>,
) -> Result<IngestResult, String> {
    let outcome = {
        let mut conn = lock(conn);
        let result = resolve_parser(&conn, &candidate.path, log_format, time_format)
            .and_then(|p| p.ok_or_else(|| UNRECOGNIZED.to_string()))
            .and_then(|parser| {
                let geo = geo.read().unwrap_or_else(|e| e.into_inner());
                ingest::ingest_file(&mut conn, &geo, &parser, candidate, opts, progress).map_err(err)
            });
        // 결과와 상관없이 -wal을 비운다. 실패해도 적재 결과는 유효하고 다음 정리(종료 시 등)에서 다시 시도된다.
        let _ = db::compact(&conn);
        result?
    };
    let (deleted, delete_error) = match (&outcome, delete_original) {
        (IngestOutcome::Ingested(_), true) => match delete(&candidate.path) {
            Ok(()) => (true, None),
            Err(e) => (false, Some(format!("원본 파일을 지우지 못했습니다: {e}"))),
        },
        _ => (false, None),
    };
    Ok(IngestResult { outcome, deleted, delete_error })
}

fn file_len(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

pub fn do_app_info(geo: &GeoDb, geo_is_bundled: bool, db_path: &Path) -> AppInfo {
    let mut wal = db_path.as_os_str().to_owned();
    wal.push("-wal");
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        geo_build_date: geo.build_date().to_string(),
        geo_is_bundled,
        db_path: db_path.display().to_string(),
        db_bytes: file_len(db_path) + file_len(Path::new(&wal)),
    }
}

/// src를 GeoIP로 열어 본 뒤 local_data_dir/geoip.mmdb로 복사하고 메모리의 DB를 바꾼다.
/// 검증·복사에 실패하면 기존 DB와 파일을 그대로 둔다.
pub fn do_replace_geoip(geo: &RwLock<GeoDb>, src: &Path, local_data_dir: &Path) -> Result<(), String> {
    let new_geo = GeoDb::open(src).map_err(|e| format!("GeoIP 파일로 열 수 없습니다: {e}"))?;
    // 복사가 중간에 끊겨도 기존 geoip.mmdb가 깨지지 않게 임시 파일에 쓴 뒤 이름을 바꾼다.
    let tmp = local_data_dir.join(format!("{LOCAL_GEOIP_FILE}.tmp"));
    let copied = std::fs::create_dir_all(local_data_dir)
        .and_then(|_| std::fs::copy(src, &tmp))
        .and_then(|_| std::fs::rename(&tmp, local_data_dir.join(LOCAL_GEOIP_FILE)));
    if let Err(e) = copied {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("GeoIP 파일을 복사하지 못했습니다: {e}"));
    }
    *geo.write().unwrap_or_else(|e| e.into_inner()) = new_geo;
    Ok(())
}

/// 현재 DB를 dest에 복사한 뒤 config.json에 기록한다. 다음 실행부터 적용된다.
pub fn do_set_db_path(conn: &Connection, dest: &Path, config_dir: &Path) -> Result<(), String> {
    db::copy_to(conn, dest).map_err(|e| format!("DB를 새 위치에 복사하지 못했습니다: {e}"))?;
    config::save(config_dir, &Config { db_path: Some(dest.to_path_buf()) }).map_err(|e| {
        let _ = std::fs::remove_file(dest);
        format!("설정 파일을 저장하지 못했습니다: {e}")
    })
}

// ---- Tauri 명령 ----

async fn run<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppState) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>()))
        .await
        .map_err(|e| format!("작업을 실행하지 못했습니다: {e}"))?
}

fn remove_original(p: &Path) -> io::Result<()> {
    std::fs::remove_file(p)
}

#[tauri::command]
pub async fn scan_paths(app: AppHandle, paths: Vec<PathBuf>) -> Result<Vec<Candidate>, String> {
    run(app, move |s| do_scan(&lock(&s.conn), &paths)).await
}

#[tauri::command]
pub async fn preview_file(
    app: AppHandle,
    path: PathBuf,
    log_format: Option<String>,
    time_format: Option<String>,
) -> Result<Preview, String> {
    run(app, move |s| do_preview(&lock(&s.conn), &path, log_format.as_deref(), time_format.as_deref())).await
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn ingest_one(
    app: AppHandle,
    candidate: Candidate,
    conflict: ConflictPolicy,
    allow_high_failure: bool,
    delete_original: bool,
    log_format: Option<String>,
    time_format: Option<String>,
) -> Result<IngestResult, String> {
    let emitter = app.clone();
    run(app, move |s| {
        let path = candidate.path.display().to_string();
        let mut progress = |bytes_read, bytes_total| {
            let _ = emitter.emit("ingest-progress", IngestProgress { path: path.clone(), bytes_read, bytes_total });
        };
        let opts = IngestOptions { conflict, allow_high_failure };
        do_ingest(
            &s.conn,
            &s.geo,
            &candidate,
            &opts,
            delete_original,
            log_format.as_deref(),
            time_format.as_deref(),
            &mut progress,
            remove_original,
        )
    })
    .await
}

#[tauri::command]
pub async fn get_kpis(app: AppHandle, filter: Filter) -> Result<Kpis, String> {
    run(app, move |s| query::kpis(&lock(&s.conn), &filter).map_err(err)).await
}

#[tauri::command]
pub async fn get_countries(app: AppHandle, filter: Filter) -> Result<Vec<CountryRow>, String> {
    run(app, move |s| query::countries(&lock(&s.conn), &filter).map_err(err)).await
}

#[tauri::command]
pub async fn get_daily(app: AppHandle, filter: Filter) -> Result<Vec<DayPoint>, String> {
    run(app, move |s| query::daily(&lock(&s.conn), &filter).map_err(err)).await
}

#[tauri::command]
pub async fn get_country_ips(
    app: AppHandle,
    filter: Filter,
    country: String,
    limit: u32,
    offset: u32,
) -> Result<Vec<IpRow>, String> {
    run(app, move |s| query::country_ips(&lock(&s.conn), &filter, &country, limit, offset).map_err(err)).await
}

#[tauri::command]
pub async fn get_ip_requests(
    app: AppHandle,
    period: Period,
    ip: String,
    limit: u32,
    offset: u32,
) -> Result<Vec<RequestRow>, String> {
    run(app, move |s| query::ip_requests(&lock(&s.conn), &period, &ip, limit, offset).map_err(err)).await
}

#[tauri::command]
pub async fn get_ingest_status(app: AppHandle) -> Result<IngestStatus, String> {
    run(app, |s| query::ingest_status(&lock(&s.conn)).map_err(err)).await
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Result<Settings, String> {
    run(app, |s| do_get_settings(&lock(&s.conn))).await
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    run(app, move |s| do_save_settings(&lock(&s.conn), &settings)).await
}

fn app_info_of(s: &AppState) -> AppInfo {
    let geo = s.geo.read().unwrap_or_else(|e| e.into_inner());
    do_app_info(&geo, s.geo_is_bundled.load(Ordering::Relaxed), &s.db_path)
}

#[tauri::command]
pub async fn get_app_info(app: AppHandle) -> Result<AppInfo, String> {
    run(app, |s| Ok(app_info_of(s))).await
}

#[tauri::command]
pub async fn replace_geoip(app: AppHandle, path: PathBuf) -> Result<AppInfo, String> {
    run(app, move |s| {
        do_replace_geoip(&s.geo, &path, &s.local_data_dir)?;
        s.geo_is_bundled.store(false, Ordering::Relaxed);
        Ok(app_info_of(s))
    })
    .await
}

/// 지운 뒤 db::compact까지 한다(purge_foreign_before 안에서).
#[tauri::command]
pub async fn purge_foreign(app: AppHandle, before_date: String) -> Result<usize, String> {
    run(app, move |s| ingest::purge_foreign_before(&lock(&s.conn), &before_date).map_err(err)).await
}

#[tauri::command]
pub async fn set_db_path(app: AppHandle, path: PathBuf) -> Result<(), String> {
    run(app, move |s| do_set_db_path(&lock(&s.conn), &path, &s.config_dir)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use iptracker_core::db;
    use iptracker_core::ingest::{scan, ConflictPolicy, IngestOptions, IngestOutcome};
    use iptracker_core::logformat::{COMBINED, COMMON};
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, RwLock};

    const GEO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/resources/dbip-country-lite.mmdb");
    const UNRECOGNIZED: &str = "로그 형식을 인식하지 못했습니다. 설정에서 LogFormat을 지정해 주세요.";

    fn deps() -> (Mutex<Connection>, RwLock<GeoDb>) {
        (Mutex::new(db::open_in_memory().unwrap()), RwLock::new(GeoDb::open(Path::new(GEO)).unwrap()))
    }
    fn write(dir: &Path, rel: &str, lines: &[String]) -> PathBuf {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        p
    }
    fn us_lines() -> Vec<String> {
        (0..10)
            .map(|i| format!(r#"8.8.8.8 - - [22/Nov/2026:10:00:00 +0900] "GET /p{i} HTTP/1.1" 200 512 "-" "Mozilla/5.0""#))
            .collect()
    }
    fn ingest(
        conn: &Mutex<Connection>,
        geo: &RwLock<GeoDb>,
        p: &Path,
        delete: fn(&Path) -> io::Result<()>,
    ) -> Result<IngestResult, String> {
        let c = scan(&[p.to_path_buf()], None).remove(0);
        let opts = IngestOptions { conflict: ConflictPolicy::Ask, allow_high_failure: false };
        do_ingest(conn, geo, &c, &opts, true, None, None, &mut |_, _| {}, delete)
    }
    fn remove(p: &Path) -> io::Result<()> {
        std::fs::remove_file(p)
    }

    #[test]
    fn ingest_deletes_original_only_when_ingested() {
        let dir = tempfile::tempdir().unwrap();
        let (conn, geo) = deps();
        let p = write(dir.path(), "was01/access.log", &us_lines());
        let r = ingest(&conn, &geo, &p, remove).unwrap();
        assert!(matches!(r.outcome, IngestOutcome::Ingested(_)), "{:?}", r.outcome);
        assert!(r.deleted);
        assert_eq!(r.delete_error, None);
        assert!(!p.exists());

        let p = write(dir.path(), "was01/access.log", &us_lines());
        let r = ingest(&conn, &geo, &p, remove).unwrap();
        assert_eq!(r.outcome, IngestOutcome::Duplicate);
        assert!(!r.deleted);
        assert!(p.exists());
    }

    #[test]
    fn delete_failure_keeps_data_and_reports() {
        let dir = tempfile::tempdir().unwrap();
        let (conn, geo) = deps();
        let p = write(dir.path(), "was01/access.log", &us_lines());
        let r = ingest(&conn, &geo, &p, |_| Err(io::Error::from(io::ErrorKind::PermissionDenied))).unwrap();
        assert!(matches!(r.outcome, IngestOutcome::Ingested(_)), "{:?}", r.outcome);
        assert!(!r.deleted);
        assert!(r.delete_error.is_some());
        assert!(p.exists());
        let rows: i64 = conn.lock().unwrap().query_row("SELECT count(*) FROM ip_daily", [], |r| r.get(0)).unwrap();
        assert!(rows > 0);
    }

    #[test]
    fn preview_and_ingest_resolve_the_same_parser() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(dir.path(), "was01/access.log", &us_lines());
        let conn = db::open_in_memory().unwrap();
        let saved = Settings { log_format: COMMON.into(), ..do_get_settings(&conn).unwrap() };
        do_save_settings(&conn, &saved).unwrap();
        let lf = |a: Option<&str>| resolve_parser(&conn, &p, a, None).unwrap().unwrap().log_format().to_string();
        assert_eq!(lf(Some(COMBINED)), COMBINED);
        assert_eq!(lf(None), COMMON);
        assert_eq!(lf(Some("")), COMMON, "빈 문자열은 미지정");

        let dotted: Vec<String> =
            (0..10).map(|i| format!(r#"8.8.8.8 - - [23.11.2026 14:00:0{i}] "GET / HTTP/1.1" 200 1"#)).collect();
        let p = write(dir.path(), "was01/dotted.log", &dotted);
        let conn = db::open_in_memory().unwrap();
        assert!(resolve_parser(&conn, &p, None, None).unwrap().is_none());
        let parser = resolve_parser(&conn, &p, None, Some("%d.%m.%Y %H:%M:%S")).unwrap().unwrap();
        assert_eq!(parser.log_format(), COMMON);
        assert_eq!(parser.parse(&dotted[0]).unwrap().ts.to_rfc3339(), "2026-11-23T14:00:00+09:00");
        // 저장된 시간 형식도 같은 규칙으로 쓰이고, 미리보기는 그 결과를 그대로 보여준다.
        let saved = Settings { time_format: "%d.%m.%Y %H:%M:%S".into(), ..do_get_settings(&conn).unwrap() };
        do_save_settings(&conn, &saved).unwrap();
        let preview = do_preview(&conn, &p, None, None).unwrap();
        assert_eq!((preview.log_format.as_deref(), preview.rows.len()), (Some(COMMON), 10));
    }

    #[test]
    fn scan_applies_saved_server_regex() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(dir.path(), "logs/was07_a.log", &us_lines());
        let conn = db::open_in_memory().unwrap();
        assert_eq!(do_scan(&conn, std::slice::from_ref(&p)).unwrap()[0].server, "logs");
        let s = Settings { server_regex: r"^(was\d+)_".into(), ..do_get_settings(&conn).unwrap() };
        do_save_settings(&conn, &s).unwrap();
        let c = do_scan(&conn, &[p]).unwrap();
        assert_eq!((c[0].server.as_str(), c[0].filename.as_str()), ("was07", "was07_a.log"));
    }

    #[test]
    fn unrecognized_format_is_a_korean_error() {
        let dir = tempfile::tempdir().unwrap();
        let (conn, geo) = deps();
        let p = write(dir.path(), "was01/garbage.log", &["hello world".to_string(), "아무 내용".to_string()]);
        assert_eq!(ingest(&conn, &geo, &p, remove).unwrap_err(), UNRECOGNIZED);
        assert!(p.exists());
        // 미리보기는 오류 대신 "형식 없음"을 돌려줘 화면이 직접 지정 칸을 띄울 수 있게 한다.
        let preview = do_preview(&conn.lock().unwrap(), &p, None, None).unwrap();
        assert_eq!((preview.log_format, preview.rows.len(), preview.sampled, preview.failed), (None, 0, 2, 2));
    }

    #[test]
    fn settings_roundtrip_and_validation() {
        let conn = db::open_in_memory().unwrap();
        let d = do_get_settings(&conn).unwrap();
        assert_eq!(
            d,
            Settings {
                service_name: String::new(),
                expected_servers: 16,
                server_regex: String::new(),
                log_format: String::new(),
                time_format: String::new(),
            }
        );
        let s = Settings {
            service_name: "민원 포털".into(),
            expected_servers: 8,
            server_regex: r"^(was\d+)_".into(),
            log_format: COMBINED.into(),
            time_format: "%d/%b/%Y:%H:%M:%S %z".into(),
        };
        do_save_settings(&conn, &s).unwrap();
        assert_eq!(do_get_settings(&conn).unwrap(), s);

        assert!(do_save_settings(&conn, &Settings { server_regex: "(".into(), ..s.clone() }).is_err());
        assert!(do_save_settings(&conn, &Settings { log_format: "%t %>s".into(), ..s.clone() }).is_err());
        assert_eq!(do_get_settings(&conn).unwrap(), s, "거부된 저장은 아무것도 바꾸지 않는다");
    }

    #[test]
    fn replace_geoip_rejects_invalid_file() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let geo = RwLock::new(GeoDb::open(Path::new(GEO)).unwrap());
        let txt = dir.path().join("fake.mmdb");
        std::fs::write(&txt, "이건 GeoIP 파일이 아니다").unwrap();
        assert!(do_replace_geoip(&geo, &txt, &data).is_err());
        assert!(!data.join("geoip.mmdb").exists());
        let us = geo.read().unwrap().classify("8.8.8.8".parse().unwrap());
        assert_eq!(us.country.as_deref(), Some("US"));

        do_replace_geoip(&geo, Path::new(GEO), &data).unwrap();
        assert!(data.join("geoip.mmdb").exists());
    }

    #[test]
    fn set_db_path_copies_then_records() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_dir = dir.path().join("cfg");
        let conn = db::open(&dir.path().join("a.db")).unwrap();
        db::set_setting(&conn, "service_name", "복사됨").unwrap();
        let dest = dir.path().join("b.db");
        do_set_db_path(&conn, &dest, &cfg_dir).unwrap();
        assert_eq!(config::load(&cfg_dir).db_path.as_deref(), Some(dest.as_path()));
        assert_eq!(db::get_setting(&db::open(&dest).unwrap(), "service_name").unwrap().as_deref(), Some("복사됨"));

        // 복사가 실패하면(대상이 이미 있음) 설정은 그대로다.
        let other = dir.path().join("a.db");
        assert!(do_set_db_path(&conn, &other, &cfg_dir).is_err());
        assert_eq!(config::load(&cfg_dir).db_path.as_deref(), Some(dest.as_path()));
    }
}
