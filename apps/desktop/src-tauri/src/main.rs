// `windows_subsystem = "windows"` keeps a console window from appearing behind
// the application window in release builds; debug builds keep the console so
// tracing output is visible during development.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // `--self-check` is the live-fire entry point: it drives the real command
    // layer inside this executable and exits before any window is created, so
    // it works on a headless machine. See `self_check.rs`.
    if args.iter().any(|arg| arg == "--self-check") {
        std::process::exit(vector_desktop_lib::self_check::run(&args));
    }

    vector_desktop_lib::run();
}
