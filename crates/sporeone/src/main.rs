#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! SporeOne's window: the catalog of Spore mods, each one's page, what is installed, and what the
//! game and the Launcher Kit need. `sporeone --apply <plan>` is the same program run with
//! administrator rights to carry out an install in the game's folder.

mod demo;
mod frame;
mod home;
mod icons;
mod library;
mod markdown;
mod page;
mod settings;
mod text_field;
mod theme;
mod ui;
mod widgets;

use gpui::{App, AppContext, Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowOptions, px, size};
use std::time::Duration;
use ui::MainView;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut demo = false;
    let mut smoke = false;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--apply" => {
                let Some(file) = args.next() else { std::process::exit(2) };
                std::process::exit(sporeone_core::actions::apply_file(std::path::Path::new(&file)));
            }
            "--demo" => demo = true,
            "--smoke" => smoke = true,
            _ => {}
        }
    }
    if demo {
        demo::sandbox();
    }

    gpui_platform::application().run(move |cx: &mut App| {
        text_field::bind_keys(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1180.), px(820.)), cx))),
            titlebar: Some(TitlebarOptions { title: Some("SporeOne".into()), ..Default::default() }),
            app_id: Some("io.github.brunovncs.sporeone".into()),
            window_min_size: Some(size(px(760.), px(540.))),
            window_background: WindowBackgroundAppearance::Opaque,
            ..Default::default()
        };
        if cx.open_window(options, |window, cx| cx.new(|cx| MainView::new(demo, window, cx))).is_err() {
            eprintln!("sporeone: could not open a window");
            std::process::exit(1);
        }
        if smoke {
            // CI: the window opened and drew; that is all a smoke run checks.
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_millis(1500)).await;
                println!("smoke: window drawn");
                std::process::exit(0);
            })
            .detach();
        }
        cx.activate(true);
    });
}
