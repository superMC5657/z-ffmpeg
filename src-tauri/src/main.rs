// 在 release 模式下阻止 Windows 弹出额外的控制台窗口，请勿删除！
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    zffmpeg_lib::run()
}
