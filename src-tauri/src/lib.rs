mod commands;
mod config;
mod state;

use tauri::{Manager, RunEvent};

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let state = state::AppState::init(app.handle())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan_paths,
            commands::preview_file,
            commands::ingest_one,
            commands::get_kpis,
            commands::get_countries,
            commands::get_daily,
            commands::get_country_ips,
            commands::get_ip_requests,
            commands::get_ingest_status,
            commands::get_settings,
            commands::save_settings,
            commands::get_app_info,
            commands::replace_geoip,
            commands::purge_foreign,
            commands::set_db_path,
        ])
        .build(tauri::generate_context!())
        .expect("IP Tracker를 시작하지 못했습니다");

    app.run(|handle, event| {
        // 종료는 process::exit로 끝나 연결이 정리되지 않으므로 여기서 -wal을 비운다.
        // 적재 중이라 연결이 잠겨 있으면 기다리지 않는다(남은 -wal은 다음 적재·정리 때 비워진다).
        if let RunEvent::Exit = event {
            if let Some(s) = handle.try_state::<state::AppState>() {
                if let Ok(conn) = s.conn.try_lock() {
                    let _ = iptracker_core::db::compact(&conn);
                }
            }
        }
    });
}
