#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod assets;
mod collect;
mod domain;
mod platform;
mod shared;
mod ui;

fn main() {
    app::run();
}
