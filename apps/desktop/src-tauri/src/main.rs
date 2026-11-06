// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `candeo --mcp` is the bridge an assistant starts: it relays bytes to the
    // running Candeo and exits, before anything of the application starts —
    // not even the single-instance plugin, which would bring the window forward.
    // A GUI program reads and writes redirected standard handles all the same.
    if std::env::args_os().nth(1).is_some_and(|arg| arg == "--mcp") {
        std::process::exit(candeo_desktop_lib::mcp_bridge());
    }
    candeo_desktop_lib::run()
}
