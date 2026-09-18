#![allow(clippy::too_many_arguments)]

mod assets;
mod clipboard;
mod core;
mod db;
mod hotkey;
mod link_preview;
mod mac;
mod model;
mod settings;
mod tray;
mod ui;
mod util;

use crate::clipboard::MonitorEvent;
use crate::core::Core;
use crate::db::Db;
use crate::settings::{data_dir, Settings};
use futures::StreamExt;
use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use gpui::{App, Application};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tray_icon::menu::MenuEvent;

enum AppEvent {
    Hotkey,
    Menu(String),
    Command(String),
}

/// Local control socket (`<data dir>/control.sock`): one command per line.
/// Commands: show, hide, toggle, prefs, preview, dump, quit, key <keystroke>.
fn spawn_control_socket(tx: futures::channel::mpsc::UnboundedSender<AppEvent>) {
    use std::io::{BufRead, BufReader};
    let path = data_dir().join("control.sock");
    let _ = std::fs::remove_file(&path);
    let listener = match std::os::unix::net::UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("control socket unavailable: {e}");
            return;
        }
    };
    std::thread::Builder::new()
        .name("control-socket".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let reader = BufReader::new(stream);
                for line in reader.lines().map_while(Result::ok) {
                    let line = line.trim().to_string();
                    if !line.is_empty() && tx.unbounded_send(AppEvent::Command(line)).is_err() {
                        return;
                    }
                }
            }
        })
        .ok();
}

static SIG_TOGGLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static SIG_PREFS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

extern "C" fn on_sigusr1(_: libc::c_int) {
    SIG_TOGGLE.store(true, std::sync::atomic::Ordering::SeqCst);
}

extern "C" fn on_sigusr2(_: libc::c_int) {
    SIG_PREFS.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// `kill -USR1 <pid>` toggles the shelf and `kill -USR2 <pid>` opens settings (handy for scripting/debugging).
fn install_signal_hooks() {
    unsafe {
        libc::signal(libc::SIGUSR1, on_sigusr1 as extern "C" fn(libc::c_int) as libc::sighandler_t);
        libc::signal(libc::SIGUSR2, on_sigusr2 as extern "C" fn(libc::c_int) as libc::sighandler_t);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--render-icon" {
        let size: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1024);
        match assets::render_app_icon(&args[2], size) {
            Ok(()) => return,
            Err(e) => {
                eprintln!("icon render failed: {e}");
                std::process::exit(1);
            }
        }
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let dir = data_dir();
    let settings = Arc::new(RwLock::new(Settings::load()));
    let db = match Db::open(dir.join("paste.db")) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Cannot open database: {e:#}");
            std::process::exit(1);
        }
    };

    let (mtx, mut mrx) = futures::channel::mpsc::unbounded::<MonitorEvent>();
    let monitor = clipboard::spawn(db.clone(), settings.clone(), mtx);
    let (etx, mut erx) = futures::channel::mpsc::unbounded::<AppEvent>();

    Application::new().with_assets(assets::Assets).run(move |cx: &mut App| {
        mac::window::set_accessory_policy();

        let mut core = match Core::load(db, settings, monitor) {
            Ok(c) => c,
            Err(e) => {
                log::error!("failed to load state: {e:#}");
                cx.quit();
                return;
            }
        };
        core.purge_expired();

        match hotkey::Hotkeys::new() {
            Ok(mut hk) => {
                let wanted = core.settings().hotkey;
                if let Err(e) = hk.set(&wanted) {
                    log::warn!("{e}; falling back to cmd-shift-v");
                    if hk.set("cmd-shift-v").is_ok() {
                        core.update_settings(|s| s.hotkey = "cmd-shift-v".into());
                    }
                }
                core.hotkeys = Some(hk);
            }
            Err(e) => log::error!("global hotkeys unavailable: {e}"),
        }
        let tx = etx.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
            if e.state == HotKeyState::Pressed {
                let _ = tx.unbounded_send(AppEvent::Hotkey);
            }
        }));

        match tray::Tray::new(&core.hotkey_display()) {
            Ok(t) => core.tray = Some(t),
            Err(e) => log::error!("menu bar item unavailable: {e}"),
        }
        let tx = etx.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let _ = tx.unbounded_send(AppEvent::Menu(e.id.0.clone()));
        }));

        let hotkey_display = core.hotkey_display();
        cx.set_global(core);

        match ui::shelf::open_shelf_window(cx) {
            Ok(handle) => cx.global_mut::<Core>().shelf = Some(handle),
            Err(e) => {
                log::error!("failed to create shelf window: {e:#}");
                cx.quit();
                return;
            }
        }

        cx.spawn(async move |cx| {
            while let Some(ev) = mrx.next().await {
                if cx.update(|cx| handle_monitor_event(ev, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();

        cx.spawn(async move |cx| {
            while let Some(ev) = erx.next().await {
                if cx.update(|cx| handle_app_event(ev, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();

        install_signal_hooks();
        spawn_control_socket(etx.clone());
        cx.spawn(async move |cx| loop {
            cx.background_executor().timer(Duration::from_millis(100)).await;
            let toggle = SIG_TOGGLE.swap(false, std::sync::atomic::Ordering::SeqCst);
            let prefs = SIG_PREFS.swap(false, std::sync::atomic::Ordering::SeqCst);
            if !toggle && !prefs {
                continue;
            }
            let ok = cx.update(|cx| {
                if toggle {
                    with_shelf(cx, |shelf, window, cx| shelf.toggle(window, cx));
                }
                if prefs {
                    ui::prefs::open_prefs(cx);
                }
            });
            if ok.is_err() {
                break;
            }
        })
        .detach();

        cx.spawn(async move |cx| loop {
            cx.background_executor().timer(Duration::from_secs(3600)).await;
            if cx.update(|cx| cx.global_mut::<Core>().purge_expired()).is_err() {
                break;
            }
        })
        .detach();

        log::info!("Paste is running in the menu bar. Press {hotkey_display} to open the shelf.");
    });
}

fn with_shelf(cx: &mut App, f: impl FnOnce(&mut ui::shelf::Shelf, &mut gpui::Window, &mut gpui::Context<ui::shelf::Shelf>)) {
    if let Some(shelf) = cx.global::<Core>().shelf {
        if let Err(e) = shelf.update(cx, f) {
            log::warn!("shelf update failed: {e}");
        }
    }
}

fn handle_monitor_event(ev: MonitorEvent, cx: &mut App) {
    {
        let core = cx.global_mut::<Core>();
        match ev {
            MonitorEvent::New(item) => core.add_item(item),
            MonitorEvent::Touched { id, created_at } => core.touch_item(id, created_at),
            MonitorEvent::LinkMeta { id, title, favicon } => core.apply_link_meta(id, title, favicon),
        }
    }
    with_shelf(cx, |shelf, _, cx| shelf.on_items_changed(cx));
}

fn handle_app_event(ev: AppEvent, cx: &mut App) {
    match ev {
        AppEvent::Hotkey => with_shelf(cx, |shelf, window, cx| shelf.toggle(window, cx)),
        AppEvent::Menu(id) => match id.as_str() {
            tray::ID_SHOW => with_shelf(cx, |shelf, window, cx| shelf.show(window, cx)),
            tray::ID_PAUSE => {
                let core = cx.global::<Core>();
                core.set_paused(!core.paused());
                cx.refresh_windows();
            }
            tray::ID_CLEAR => with_shelf(cx, |shelf, window, cx| shelf.show_and_confirm_clear(window, cx)),
            tray::ID_PREFS => ui::prefs::open_prefs(cx),
            tray::ID_QUIT => cx.quit(),
            other => log::debug!("unhandled menu id {other}"),
        },
        AppEvent::Command(cmd) => handle_command(&cmd, cx),
    }
}

fn handle_command(cmd: &str, cx: &mut App) {
    let (verb, arg) = cmd.split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((cmd, ""));
    match verb {
        "show" => with_shelf(cx, |s, w, cx| s.show(w, cx)),
        "hide" => with_shelf(cx, |s, w, cx| s.hide(w, cx)),
        "toggle" => with_shelf(cx, |s, w, cx| s.toggle(w, cx)),
        "prefs" => ui::prefs::open_prefs(cx),
        "preview" => with_shelf(cx, |s, w, cx| s.debug_toggle_preview(w, cx)),
        "dump" => {
            let ax = mac::paste::accessibility_trusted(false);
            with_shelf(cx, move |s, _, _| log::info!("state: {} accessibility={}", s.debug_state(), ax));
        }
        "quit" => cx.quit(),
        "shot" => {
            // Saves PNGs of every open Paste window: <arg>-shelf.png, <arg>-preview.png, <arg>-prefs.png
            let prefix = if arg.is_empty() { "/tmp/paste".to_string() } else { arg.to_string() };
            let shelf_win = cx.global::<Core>().shelf.and_then(|h| h.update(cx, |s, _, _| s.native_window()).ok().flatten());
            let mut targets: Vec<(String, Option<objc2::rc::Retained<objc2_app_kit::NSWindow>>)> = vec![("shelf".into(), shelf_win)];
            if let Some(h) = cx.global::<Core>().preview {
                targets.push(("preview".into(), h.update(cx, |_, w, _| mac::window::ns_window(w)).ok().flatten()));
            }
            if let Some(h) = cx.global::<Core>().prefs {
                targets.push(("prefs".into(), h.update(cx, |_, w, _| mac::window::ns_window(w)).ok().flatten()));
            }
            cx.spawn(async move |_| {
                for (name, win) in targets {
                    if let Some(win) = win {
                        let path = std::path::PathBuf::from(format!("{prefix}-{name}.png"));
                        match mac::window::capture_png(&win, &path) {
                            Ok(()) => log::info!("saved {}", path.display()),
                            Err(e) => log::warn!("capture {name} failed: {e}"),
                        }
                    }
                }
            })
            .detach();
        }
        "key" => match gpui::Keystroke::parse(arg) {
            Ok(mut ks) => {
                if ks.key_char.is_none() && ks.key.chars().count() == 1 && !ks.modifiers.platform && !ks.modifiers.control {
                    ks.key_char = Some(if ks.modifiers.shift { ks.key.to_uppercase() } else { ks.key.clone() });
                }
                let event = gpui::KeyDownEvent { keystroke: ks, is_held: false };
                with_shelf(cx, move |s, w, cx| s.handle_key(&event, w, cx));
            }
            Err(e) => log::warn!("bad keystroke {arg:?}: {e}"),
        },
        other => log::warn!("unknown command {other:?}"),
    }
}
