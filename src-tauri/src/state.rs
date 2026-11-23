//! 앱 전체가 공유하는 상태: DB 연결, GeoIP DB, 파일 위치.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
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

pub struct AppState {
    pub conn: Mutex<Connection>,
    pub geo: RwLock<GeoDb>,
    /// 내장 GeoIP를 쓰는 중인지(교체하면 false)
    pub geo_is_bundled: AtomicBool,
    pub db_path: PathBuf,
    /// app_local_data_dir (Roaming 아님)
    pub local_data_dir: PathBuf,
    /// config.json이 있는 app_config_dir
    pub config_dir: PathBuf,
}

/// 앞선 작업이 패닉으로 끝났어도 연결은 계속 쓴다(열린 트랜잭션은 drop에서 되돌려진다).
pub fn lock(conn: &Mutex<Connection>) -> MutexGuard<'_, Connection> {
    conn.lock().unwrap_or_else(PoisonError::into_inner)
}

impl AppState {
    pub fn init(app: &AppHandle) -> Result<AppState, String> {
        let paths = app.path();
        let local_data_dir = paths.app_local_data_dir().map_err(|e| format!("앱 데이터 폴더를 찾지 못했습니다: {e}"))?;
        let config_dir = paths.app_config_dir().map_err(|e| format!("앱 설정 폴더를 찾지 못했습니다: {e}"))?;
        std::fs::create_dir_all(&local_data_dir).map_err(|e| format!("앱 데이터 폴더를 만들지 못했습니다: {e}"))?;

        // 지정한 위치(이동식 디스크 등)를 열 수 없으면 기본 위치로 시작해 설정 화면에서 고칠 수 있게 한다.
        let default_db = local_data_dir.join(DB_FILE);
        let configured = config::load(&config_dir).db_path.and_then(|p| match db::open(&p) {
            Ok(c) => Some((c, p)),
            Err(e) => {
                eprintln!("지정한 DB {}를 열지 못해 기본 위치를 씁니다: {e}", p.display());
                None
            }
        });
        let (conn, db_path) = match configured {
            Some(opened) => opened,
            None => (db::open(&default_db).map_err(|e| e.to_string())?, default_db),
        };

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
            conn: Mutex::new(conn),
            geo: RwLock::new(geo),
            geo_is_bundled: AtomicBool::new(geo_is_bundled),
            db_path,
            local_data_dir,
            config_dir,
        })
    }
}
