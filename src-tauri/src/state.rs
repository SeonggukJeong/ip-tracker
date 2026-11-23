//! 앱 전체가 공유하는 상태: DB 연결, GeoIP DB, 파일 위치.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError, RwLock};

use iptracker_core::db;
use iptracker_core::geo::GeoDb;
use rusqlite::Connection;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};

use crate::config;

const DB_FILE: &str = "ip-tracker.db";
pub const LOCAL_GEOIP_FILE: &str = "geoip.mmdb";
const BUNDLED_GEOIP: &str = "resources/dbip-country-lite.mmdb";
pub const DB_MOVED: &str = "DB 위치를 바꿨습니다. 앱을 다시 시작해 주세요.";

/// DB 연결과 "DB 위치를 바꿈" 표시.
/// set_db_path 뒤의 쓰기는 다음 실행에서 열지 않는 옛 DB에 들어가 사라지므로 거부한다.
pub struct Db {
    conn: Mutex<Connection>,
    moved: AtomicBool,
}

impl Db {
    pub fn new(conn: Connection) -> Db {
        Db { conn: Mutex::new(conn), moved: AtomicBool::new(false) }
    }

    /// 읽기만 하는 명령용. 앞선 작업이 패닉으로 끝났어도 연결은 계속 쓴다(열린 트랜잭션은 drop에서 되돌려진다).
    pub fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 쓰는 명령용. DB 위치를 바꾼 뒤에는 거부한다. 표시는 잠금을 잡은 채로 확인한다.
    pub fn lock_for_write(&self) -> Result<MutexGuard<'_, Connection>, String> {
        let conn = self.lock();
        if self.moved.load(Ordering::SeqCst) {
            return Err(DB_MOVED.to_string());
        }
        Ok(conn)
    }

    /// lock_for_write로 얻은 잠금을 잡은 채로 부른다.
    pub fn mark_moved(&self) {
        self.moved.store(true, Ordering::SeqCst);
    }

    /// 기다리지 않는 잠금(종료 처리용).
    pub fn try_lock(&self) -> Option<MutexGuard<'_, Connection>> {
        self.conn.try_lock().ok()
    }
}

pub struct AppState {
    pub db: Db,
    pub geo: RwLock<GeoDb>,
    /// 내장 GeoIP를 쓰는 중인지(교체하면 false)
    pub geo_is_bundled: AtomicBool,
    pub db_path: PathBuf,
    /// 설정된 DB를 열지 못해 기본 위치를 쓰는 경우의 안내
    pub db_warning: Option<String>,
    /// app_local_data_dir (Roaming 아님)
    pub local_data_dir: PathBuf,
    /// config.json이 있는 app_config_dir
    pub config_dir: PathBuf,
}

/// config.json의 dbPath를 연다. 파일이 없거나 열 수 없으면(이동식 디스크 분리 등) 기본 위치의 DB를 열고
/// 안내 문구를 함께 돌려준다. 없는 파일을 새로 만들지 않도록 존재부터 확인한다.
pub fn open_db(config_dir: &Path, local_data_dir: &Path) -> Result<(Connection, PathBuf, Option<String>), String> {
    let mut warning = None;
    if let Some(p) = config::load(config_dir).db_path {
        if p.is_file() {
            if let Ok(conn) = db::open(&p) {
                return Ok((conn, p, None));
            }
        }
        warning = Some(format!("설정된 DB({})를 열지 못해 기본 위치의 DB를 사용합니다.", p.display()));
    }
    std::fs::create_dir_all(local_data_dir).map_err(|e| format!("앱 데이터 폴더를 만들지 못했습니다: {e}"))?;
    let default_db = local_data_dir.join(DB_FILE);
    let conn = db::open(&default_db).map_err(|e| e.to_string())?;
    Ok((conn, default_db, warning))
}

impl AppState {
    pub fn init(app: &AppHandle) -> Result<AppState, String> {
        let paths = app.path();
        let local_data_dir = paths.app_local_data_dir().map_err(|e| format!("앱 데이터 폴더를 찾지 못했습니다: {e}"))?;
        let config_dir = paths.app_config_dir().map_err(|e| format!("앱 설정 폴더를 찾지 못했습니다: {e}"))?;

        let (conn, db_path, db_warning) = open_db(&config_dir, &local_data_dir)?;

        let local_geo = local_data_dir.join(LOCAL_GEOIP_FILE);
        let replaced = if local_geo.exists() {
            GeoDb::open(&local_geo)
                .inspect_err(|e| eprintln!("교체한 GeoIP {}를 열지 못해 내장 DB를 씁니다: {e}", local_geo.display()))
                .ok()
        } else {
            None
        };
        let geo_is_bundled = replaced.is_none();
        let geo = match replaced {
            Some(g) => g,
            None => {
                let bundled = paths
                    .resolve(BUNDLED_GEOIP, BaseDirectory::Resource)
                    .map_err(|e| format!("내장 GeoIP 위치를 찾지 못했습니다: {e}"))?;
                GeoDb::open(&bundled).map_err(|e| format!("내장 GeoIP를 열지 못했습니다: {e}"))?
            }
        };

        Ok(AppState {
            db: Db::new(conn),
            geo: RwLock::new(geo),
            geo_is_bundled: AtomicBool::new(geo_is_bundled),
            db_path,
            db_warning,
            local_data_dir,
            config_dir,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn open_db_uses_configured_file_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let (cfg, local) = (dir.path().join("cfg"), dir.path().join("local"));
        let (conn, default_path, warning) = open_db(&cfg, &local).unwrap();
        assert_eq!((default_path, warning), (local.join(DB_FILE), None));
        drop(conn);

        let moved = dir.path().join("moved.db");
        drop(db::open(&moved).unwrap());
        config::save(&cfg, &Config { db_path: Some(moved.clone()) }).unwrap();
        let (_, path, warning) = open_db(&cfg, &local).unwrap();
        assert_eq!((path, warning), (moved, None));
    }

    #[test]
    fn open_db_falls_back_when_configured_file_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let (cfg, local) = (dir.path().join("cfg"), dir.path().join("local"));
        let gone = dir.path().join("gone.db");
        config::save(&cfg, &Config { db_path: Some(gone.clone()) }).unwrap();
        let (_, path, warning) = open_db(&cfg, &local).unwrap();
        assert_eq!(path, local.join(DB_FILE));
        assert_eq!(warning, Some(format!("설정된 DB({})를 열지 못해 기본 위치의 DB를 사용합니다.", gone.display())));
        assert!(!gone.exists(), "없는 DB 파일을 새로 만들지 않는다");
    }
}
