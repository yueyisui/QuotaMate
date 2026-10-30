// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
mod linux_startup;

fn main() {
    #[cfg(target_os = "linux")]
    linux_startup::initialize();

    quotamate_lib::run()
}
