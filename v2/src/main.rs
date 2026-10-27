#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod native_ui;
mod view_state;

#[cfg(windows)]
fn main() {
    if let Err(error) = native_ui::run() {
        native_ui::show_startup_error(&error);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("VarKeep requires Windows 10 or later.");
}
