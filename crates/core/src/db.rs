//! SQLite 스키마, 마이그레이션, 설정 값.

use crate::Error;
use rusqlite::Connection;
use std::path::Path;

pub const SCHEMA_VERSION: i32 = 1;

/// 인덱스 i의 항목이 버전 i+1로 올리는 SQL. 2·3단계에서 항목을 추가한다.
const MIGRATIONS: &[&str] = &[r#"
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
"#];

const _: () = assert!(MIGRATIONS.len() == SCHEMA_VERSION as usize);

pub fn open(path: &Path) -> Result<Connection, Error> {
    init(Connection::open(path)?)
}

pub fn open_in_memory() -> Result<Connection, Error> {
    init(Connection::open_in_memory()?)
}

fn init(conn: Connection) -> Result<Connection, Error> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    let current: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current == 0 {
        // 첫 테이블(·WAL 헤더)을 쓰기 전에만 켤 수 있으므로 journal_mode보다 먼저 한다.
        conn.pragma_update(None, "auto_vacuum", "INCREMENTAL")?;
    }
    // journal_mode는 결과 행을 돌려준다(메모리 DB는 "memory").
    conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i as i32 + 1)?;
        tx.commit()?;
    }
    Ok(conn)
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>, Error> {
    match conn.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| r.get(0)) {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [key, value],
    )?;
    Ok(())
}

/// PRAGMA incremental_vacuum 후 wal_checkpoint(TRUNCATE). 삭제·대체·적재 뒤에 호출해 파일 크기를 실제로 줄인다.
pub fn compact(conn: &Connection) -> Result<(), Error> {
    let mut st = conn.prepare("PRAGMA incremental_vacuum")?;
    let mut rows = st.query([])?;
    while rows.next()?.is_some() {}
    drop(rows);
    drop(st);
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    Ok(())
}

/// VACUUM INTO로 현재 DB를 dest에 복사한다(WAL 내용 포함). dest가 이미 있으면 오류.
pub fn copy_to(conn: &Connection, dest: &Path) -> Result<(), Error> {
    conn.execute("VACUUM INTO ?1", [dest.to_string_lossy().as_ref()])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
