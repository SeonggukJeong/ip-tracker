// 릴리즈 빌드에서 Windows 콘솔 창이 뜨지 않게 한다.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ip_tracker_lib::run()
}
