// Everything worth asserting lives in `lib.rs`, which is testable without a
// webview. This file is the entry point and nothing else.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    readingbuddy_gui_lib::run()
}
