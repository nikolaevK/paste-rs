//! The bottom shelf: Paste's main panel with pinboard tabs, search and the horizontal card strip.
use crate::assets::icon;
use crate::core::{AppIcon, Core};
use crate::db::now_ms;
use crate::mac;
use crate::mac::pasteboard::WriteItem;
use crate::model::{ClipItem, ItemKind, PINBOARD_COLORS};
use crate::ui::card::{render_card, DragGhost, CARD_GAP, CARD_W};
use crate::ui::theme::{hex_to_hsla, Theme};
use crate::ui::widgets::icon_button;
use gpui::{prelude::*, *};
use objc2::rc::Retained;
use objc2_app_kit::NSWindow;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

pub const SHELF_H: f32 = 308.0;
pub const TOPBAR_H: f32 = 48.0;
const PAD_X: f32 = 20.0;
const STRIP_TOP: f32 = 6.0;
const STRIDE: f32 = CARD_W + CARD_GAP;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    History,
    Pinboard(i64),
}

#[derive(Clone, Debug)]
enum MenuTarget {
    Item(usize),
    Pinboard(i64),
}

struct MenuState {
    target: MenuTarget,
    position: Point<Pixels>,
    /// Keyboard-highlighted entry.
    cursor: Option<usize>,
}

#[derive(Clone, Debug)]
enum MenuAction {
    Paste,
    PastePlain,
    Copy,
    Preview,
    OpenLink,
    Reveal,
    AddToPinboard(i64),
    NewPinboardWith,
    Delete,
    RenamePinboard(i64),
    RecolorPinboard(i64),
    DeletePinboard(i64),
}

struct MenuEntry {
    label: SharedString,
    action: MenuAction,
    separator_before: bool,
    shortcut: Option<&'static str>,
    dot: Option<Hsla>,
}

#[derive(Clone)]
pub struct DragItem {
    pub ids: Vec<i64>,
}

enum Editing {
    None,
    NewPinboard(String),
    Rename(i64, String),
}

pub struct Shelf {
    ns_window: Option<Retained<NSWindow>>,
    focus_handle: FocusHandle,
    scroll: ScrollHandle,
    tab: Tab,
    query: String,
    search_active: bool,
    visible: Vec<Arc<ClipItem>>,
    selected: usize,
    multi: BTreeSet<usize>,
    anchor: usize,
    hovered: Option<usize>,
    show_gen: u64,
    pub shown: bool,
    suppress_hide: bool,
    menu: Option<MenuState>,
    editing: Editing,
    toast: Option<SharedString>,
    toast_gen: u64,
    confirm_clear: bool,
    now: i64,
    _tick: Task<()>,
}

pub fn open_shelf_window(cx: &mut App) -> anyhow::Result<WindowHandle<Shelf>> {
    let screen = mac::screen::screens().into_iter().next();
    let (w, h) = screen.map(|s| (s.width as f32, s.height as f32)).unwrap_or((1440., 900.));
    let bounds = Bounds::new(point(px(0.), px(h - SHELF_H)), size(px(w), px(SHELF_H)));
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: false,
            show: false,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            display_id: None,
            window_background: WindowBackgroundAppearance::Blurred,
            app_id: None,
            window_min_size: None,
            window_decorations: None,
            tabbing_identifier: None,
        },
        |window, cx| cx.new(|cx| Shelf::new(window, cx)),
    )?;
    // Configure the native panel outside the update cycle (style mask changes notify AppKit observers).
    let win = handle.update(cx, |shelf, _, _| shelf.ns_window.clone())?;
    cx.spawn(async move |_| {
        if let Some(win) = win {
            mac::window::configure_shelf(&win);
        }
    })
    .detach();
    Ok(handle)
}

impl Shelf {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        cx.observe_window_activation(window, |this, window, cx| {
            let active = window.is_window_active();
            if active && cx.global::<Core>().preview.is_some() {
                // The user clicked back on the shelf while the preview was open: dismiss the preview.
                this.close_preview(cx);
            }
            if !active && this.shown && !this.suppress_hide {
                this.hide(window, cx);
            }
        })
        .detach();
        let tick = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(30)).await;
            let alive = this
                .update(cx, |this, cx| {
                    if this.shown {
                        this.now = now_ms();
                        cx.notify();
                    }
                })
                .is_ok();
            if !alive {
                break;
            }
        });
        let mut shelf = Shelf {
            ns_window: mac::window::ns_window(window),
            focus_handle,
            scroll: ScrollHandle::new(),
            tab: Tab::History,
            query: String::new(),
            search_active: false,
            visible: Vec::new(),
            selected: 0,
            multi: BTreeSet::new(),
            anchor: 0,
            hovered: None,
            show_gen: 0,
            shown: false,
            suppress_hide: false,
            menu: None,
            editing: Editing::None,
            toast: None,
            toast_gen: 0,
            confirm_clear: false,
            now: now_ms(),
            _tick: tick,
        };
        shelf.refresh(cx);
        shelf
    }

    // ---------- data ----------

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if let Tab::Pinboard(id) = self.tab {
            if !cx.global::<Core>().pinboards.iter().any(|p| p.id == id) {
                self.tab = Tab::History;
            }
        }
        let core = cx.global::<Core>();
        let source: Vec<Arc<ClipItem>> = match self.tab {
            Tab::History => core.history.clone(),
            Tab::Pinboard(id) => core.pinned.get(&id).cloned().unwrap_or_default(),
        };
        let q = self.query.trim();
        if q.is_empty() {
            self.visible = source;
        } else {
            let ids: std::collections::HashSet<i64> = core.db.search_ids(q).unwrap_or_default().into_iter().collect();
            let ql = q.to_lowercase();
            self.visible = source
                .into_iter()
                .filter(|i| {
                    ids.contains(&i.id)
                        || i.preview.to_lowercase().contains(&ql)
                        || i.title.as_ref().map(|t| t.to_lowercase().contains(&ql)).unwrap_or(false)
                        || i.app_name.to_lowercase().contains(&ql)
                        || i.kind.label().to_lowercase() == ql
                })
                .collect();
        }
        if self.visible.is_empty() {
            self.selected = 0;
            self.multi.clear();
        } else if self.selected >= self.visible.len() {
            self.selected = self.visible.len() - 1;
        }
        self.multi.retain(|&i| i < self.visible.len());
        cx.notify();
    }

    pub fn on_items_changed(&mut self, cx: &mut Context<Self>) {
        if !self.shown {
            return;
        }
        let keep_id = self.visible.get(self.selected).map(|i| i.id);
        let anchor_id = self.visible.get(self.anchor).map(|i| i.id);
        let multi_ids: Vec<i64> = self.multi.iter().filter_map(|&i| self.visible.get(i).map(|x| x.id)).collect();
        self.refresh(cx);
        // Keep the same logical items selected when the list shifts (a new item arrived on top).
        let pos_of = |id: i64, v: &[Arc<ClipItem>]| v.iter().position(|i| i.id == id);
        if let Some(id) = keep_id {
            if self.selected != 0 {
                if let Some(pos) = pos_of(id, &self.visible) {
                    self.selected = pos;
                }
            }
        }
        if let Some(id) = anchor_id {
            if let Some(pos) = pos_of(id, &self.visible) {
                self.anchor = pos;
            }
        }
        if !multi_ids.is_empty() {
            self.multi = multi_ids.iter().filter_map(|&id| pos_of(id, &self.visible)).collect();
            if self.multi.len() <= 1 {
                self.multi.clear();
            }
        }
    }

    fn selected_ids(&self) -> Vec<i64> {
        self.selected_items().iter().map(|i| i.id).collect()
    }

    fn current_item(&self) -> Option<Arc<ClipItem>> {
        self.visible.get(self.selected).cloned()
    }

    fn selected_indexes(&self) -> Vec<usize> {
        if self.multi.is_empty() {
            if self.visible.is_empty() { vec![] } else { vec![self.selected] }
        } else {
            let mut v: Vec<usize> = self.multi.iter().copied().collect();
            if !v.contains(&self.selected) && self.selected < self.visible.len() {
                v.push(self.selected);
                v.sort_unstable();
            }
            v
        }
    }

    fn selected_items(&self) -> Vec<Arc<ClipItem>> {
        self.selected_indexes().into_iter().filter_map(|i| self.visible.get(i).cloned()).collect()
    }

    // ---------- visibility ----------

    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let own_pid = cx.global::<Core>().own_pid;
        if let Some(front) = mac::workspace::frontmost_app() {
            // If Paste itself is frontmost (e.g. Settings is focused) there is no paste target.
            cx.global_mut::<Core>().front_app = if front.pid != own_pid { Some(front) } else { None };
        }
        let screen = mac::screen::screen_under_mouse();
        let win = self.ns_window.clone();
        // Native window calls must run outside gpui's update cycle: AppKit delivers window
        // notifications synchronously and gpui needs to borrow the app to handle them.
        cx.spawn_in(window, async move |this, cx| {
            if let Some(win) = win {
                match screen {
                    Some(s) => mac::window::show_at(&win, s.cocoa_x, s.cocoa_y, s.width, SHELF_H as f64),
                    None => win.makeKeyAndOrderFront(None),
                }
                log::debug!("shelf shown on {:?}: {}", screen, mac::window::debug_frame(&win));
            }
            this.update_in(cx, |this, window, cx| {
                window.focus(&this.focus_handle);
                cx.notify();
            })
            .ok();
        })
        .detach();
        self.query.clear();
        self.search_active = false;
        self.selected = 0;
        self.anchor = 0;
        self.multi.clear();
        self.menu = None;
        self.editing = Editing::None;
        self.confirm_clear = false;
        self.hovered = None;
        self.scroll.set_offset(point(px(0.), px(0.)));
        self.now = now_ms();
        self.show_gen += 1;
        self.shown = true;
        self.suppress_hide = false;
        self.refresh(cx);
        cx.notify();
    }

    pub fn hide(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.shown {
            return;
        }
        self.shown = false;
        self.menu = None;
        self.editing = Editing::None;
        self.confirm_clear = false;
        self.close_preview(cx);
        let win = self.ns_window.clone();
        cx.spawn(async move |_, _| {
            if let Some(win) = win {
                mac::window::hide(&win);
            }
        })
        .detach();
        cx.notify();
    }

    pub fn debug_state(&self) -> String {
        format!(
            "shown={} tab={:?} query={:?} search_active={} visible={} selected={} multi={:?} menu={} editing={} preview_suppress={} scroll_x={}",
            self.shown,
            self.tab,
            self.query,
            self.search_active,
            self.visible.len(),
            self.selected,
            self.multi,
            self.menu.is_some(),
            !matches!(self.editing, Editing::None),
            self.suppress_hide,
            f32::from(self.scroll.offset().x),
        )
    }

    pub fn handle_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.on_key_down(event, window, cx);
    }

    pub fn debug_toggle_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_preview(window, cx);
    }

    pub fn native_window(&self) -> Option<Retained<NSWindow>> {
        self.ns_window.clone()
    }

    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.shown {
            self.hide(window, cx);
        } else {
            self.show(window, cx);
        }
    }

    pub fn show_and_confirm_clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.shown {
            self.show(window, cx);
        }
        self.tab = Tab::History;
        self.refresh(cx);
        self.confirm_clear = true;
        cx.notify();
    }

    fn toast(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.toast = Some(message.into());
        self.toast_gen += 1;
        let gen = self.toast_gen;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1600)).await;
            this.update(cx, |this, cx| {
                if this.toast_gen == gen {
                    this.toast = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    // ---------- selection ----------

    fn view_width(&self, window: &Window) -> f32 {
        f32::from(window.viewport_size().width) - 2.0 * PAD_X
    }

    fn ensure_visible(&mut self, window: &Window) {
        if self.visible.is_empty() {
            return;
        }
        let view_w = self.view_width(window);
        let x0 = self.selected as f32 * STRIDE;
        let x1 = x0 + CARD_W;
        let off = -f32::from(self.scroll.offset().x);
        let new_off = if x0 < off {
            Some(x0)
        } else if x1 > off + view_w {
            Some(x1 - view_w)
        } else {
            None
        };
        if let Some(o) = new_off {
            let max = (self.visible.len() as f32 * STRIDE - CARD_GAP - view_w).max(0.0);
            self.scroll.set_offset(point(px(-o.clamp(0.0, max)), px(0.)));
        }
    }

    fn select(&mut self, idx: usize, extend: bool, toggle: bool, window: &Window, cx: &mut Context<Self>) {
        if self.visible.is_empty() {
            return;
        }
        let idx = idx.min(self.visible.len() - 1);
        if toggle {
            if self.multi.is_empty() {
                self.multi.insert(self.selected);
            }
            if self.multi.contains(&idx) && self.multi.len() > 1 {
                self.multi.remove(&idx);
                self.selected = *self.multi.iter().next_back().unwrap_or(&idx);
                if self.multi.len() == 1 {
                    self.multi.clear();
                }
            } else {
                self.multi.insert(idx);
                self.selected = idx;
            }
        } else if extend {
            let (a, b) = if self.anchor <= idx { (self.anchor, idx) } else { (idx, self.anchor) };
            self.multi = (a..=b).collect();
            self.selected = idx;
        } else {
            self.multi.clear();
            self.selected = idx;
            self.anchor = idx;
        }
        self.ensure_visible(window);
        cx.notify();
    }

    fn move_selection(&mut self, delta: i64, extend: bool, window: &Window, cx: &mut Context<Self>) {
        if self.visible.is_empty() {
            return;
        }
        let target = (self.selected as i64 + delta).clamp(0, self.visible.len() as i64 - 1) as usize;
        self.select(target, extend, false, window, cx);
    }

    fn page_size(&self, window: &Window) -> i64 {
        ((self.view_width(window) / STRIDE).floor() as i64).max(1)
    }

    // ---------- tabs ----------

    fn tabs(&self, cx: &App) -> Vec<Tab> {
        let mut tabs = vec![Tab::History];
        tabs.extend(cx.global::<Core>().pinboards.iter().map(|p| Tab::Pinboard(p.id)));
        tabs
    }

    fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.editing = Editing::None;
        if self.tab != tab {
            self.tab = tab;
            self.selected = 0;
            self.anchor = 0;
            self.multi.clear();
            self.scroll.set_offset(point(px(0.), px(0.)));
            self.refresh(cx);
        }
    }

    fn cycle_tab(&mut self, delta: i64, cx: &mut Context<Self>) {
        let tabs = self.tabs(cx);
        let cur = tabs.iter().position(|t| *t == self.tab).unwrap_or(0) as i64;
        let next = (cur + delta).rem_euclid(tabs.len() as i64) as usize;
        self.set_tab(tabs[next], cx);
    }

    // ---------- actions ----------

    fn write_items_to_pasteboard(&self, items: &[Arc<ClipItem>], plain: bool, cx: &App) -> Option<isize> {
        let core = cx.global::<Core>();
        let db = &core.db;
        if items.is_empty() {
            return None;
        }
        let marker = items[0].id;
        // clearContents bumps the change count immediately; claim it before writing so the
        // monitor never sees a half-written pasteboard as an external copy.
        core.monitor
            .own_change
            .store(mac::pasteboard::change_count() + 1, std::sync::atomic::Ordering::Relaxed);
        if items.len() == 1 {
            let item = &items[0];
            let payload = db.payload(item.id).ok()?;
            let count = match item.kind {
                ItemKind::Image => {
                    let bytes = item.image_path.as_ref().and_then(|p| std::fs::read(p).ok())?;
                    mac::pasteboard::write(WriteItem::Image { png: &bytes }, marker)
                }
                ItemKind::File => mac::pasteboard::write(WriteItem::Files(&item.files), marker),
                ItemKind::RichText if !plain => mac::pasteboard::write(
                    WriteItem::Rich { text: &payload.text, rtf: payload.rtf.as_deref(), html: payload.html.as_deref() },
                    marker,
                ),
                _ => mac::pasteboard::write(WriteItem::Text(&payload.text), marker),
            };
            Some(count)
        } else {
            let mut parts = Vec::new();
            for item in items {
                match item.kind {
                    ItemKind::Image => parts.push(item.preview.clone()),
                    ItemKind::File => parts.extend(item.files.iter().map(|f| f.to_string_lossy().to_string())),
                    _ => {
                        if let Ok(p) = db.payload(item.id) {
                            parts.push(p.text);
                        }
                    }
                }
            }
            let joined = parts.join("\n");
            Some(mac::pasteboard::write(WriteItem::Text(&joined), marker))
        }
    }

    fn after_write(&mut self, items: &[Arc<ClipItem>], change_count: isize, cx: &mut Context<Self>) {
        let core = cx.global_mut::<Core>();
        core.monitor.own_change.store(change_count, std::sync::atomic::Ordering::Relaxed);
        if core.settings.read().unwrap().move_pasted_to_top && self.tab == Tab::History {
            for item in items.iter().rev() {
                if let Ok(created) = core.db.touch(item.id) {
                    core.touch_item(item.id, created);
                }
            }
        }
    }

    fn paste_selected(&mut self, plain: bool, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.selected_items();
        if items.is_empty() {
            return;
        }
        let plain = plain || cx.global::<Core>().settings.read().unwrap().paste_plain_default;
        let Some(count) = self.write_items_to_pasteboard(&items, plain, cx) else {
            self.toast("Couldn't load item", cx);
            return;
        };
        self.after_write(&items, count, cx);
        let front = cx.global::<Core>().front_app.clone();
        let own_pid = cx.global::<Core>().own_pid;
        let trusted = mac::paste::accessibility_trusted(false);
        if !trusted {
            let core = cx.global_mut::<Core>();
            if !core.ax_prompted {
                core.ax_prompted = true;
                mac::paste::accessibility_trusted(true);
            }
            // Keep the shelf open so the explanation is actually visible.
            self.toast("Copied — grant Accessibility access to paste directly", cx);
            if self.tab == Tab::History {
                self.refresh(cx);
            }
            return;
        }
        if front.is_none() {
            self.toast("Copied", cx);
            return;
        }
        self.hide(window, cx);
        if self.tab == Tab::History {
            self.refresh(cx);
        }
        cx.spawn(async move |_, cx| {
            cx.background_executor().timer(Duration::from_millis(50)).await;
            if let Some(front) = front {
                if front.pid != own_pid && !mac::workspace::is_pid_active(front.pid) {
                    mac::workspace::activate_pid(front.pid);
                    cx.background_executor().timer(Duration::from_millis(180)).await;
                }
            }
            mac::paste::send_paste_keystroke(false);
        })
        .detach();
    }

    fn copy_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.selected_items();
        if items.is_empty() {
            return;
        }
        if let Some(count) = self.write_items_to_pasteboard(&items, false, cx) {
            self.after_write(&items, count, cx);
            let close = cx.global::<Core>().settings.read().unwrap().close_after_paste;
            if close {
                self.hide(window, cx);
            } else {
                self.toast("Copied", cx);
            }
            self.refresh(cx);
        }
    }

    fn delete_selected(&mut self, cx: &mut Context<Self>) {
        let items = self.selected_items();
        if items.is_empty() {
            return;
        }
        let first = self.selected_indexes().first().copied().unwrap_or(0);
        {
            let core = cx.global_mut::<Core>();
            for item in &items {
                if let Err(e) = core.delete_item(item.id) {
                    log::warn!("delete failed: {e}");
                }
            }
        }
        self.multi.clear();
        self.refresh(cx);
        self.selected = first.min(self.visible.len().saturating_sub(1));
        self.anchor = self.selected;
        cx.notify();
    }

    fn pin_selected_to(&mut self, pinboard_id: i64, cx: &mut Context<Self>) {
        let items = self.selected_items();
        let mut n = 0;
        {
            let core = cx.global_mut::<Core>();
            for item in &items {
                if core.is_pinned_in(pinboard_id, &item.hash) {
                    continue;
                }
                if core.pin_item(item.id, pinboard_id).is_ok() {
                    n += 1;
                }
            }
        }
        let name = cx
            .global::<Core>()
            .pinboards
            .iter()
            .find(|p| p.id == pinboard_id)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        if n > 0 {
            self.toast(format!("Added to {name}"), cx);
        }
        if self.tab == Tab::Pinboard(pinboard_id) {
            self.refresh(cx);
        }
        cx.notify();
    }

    fn open_selected_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(item) = self.current_item() {
            if item.kind == ItemKind::Link {
                mac::workspace::open_url(item.preview.trim());
                self.hide(window, cx);
            }
        }
    }

    fn reveal_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(item) = self.current_item() {
            if item.kind == ItemKind::File {
                mac::workspace::reveal_in_finder(&item.files);
            } else if let Some(p) = &item.image_path {
                mac::workspace::reveal_in_finder(std::slice::from_ref(p));
            }
            self.hide(window, cx);
        }
    }

    fn clear_history(&mut self, cx: &mut Context<Self>) {
        if let Err(e) = cx.global_mut::<Core>().clear_history() {
            log::warn!("clear history failed: {e}");
        }
        self.confirm_clear = false;
        self.multi.clear();
        self.selected = 0;
        self.refresh(cx);
        self.toast("Clipboard history cleared", cx);
    }

    // ---------- preview ----------

    fn close_preview(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = cx.global_mut::<Core>().preview.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
        self.suppress_hide = false;
    }

    fn toggle_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if cx.global::<Core>().preview.is_some() {
            self.close_preview(cx);
            window.activate_window();
            window.focus(&self.focus_handle);
            return;
        }
        let Some(item) = self.current_item() else { return };
        self.suppress_hide = true;
        let app_icon = cx.global_mut::<Core>().icon_for(&item.app_bundle);
        match crate::ui::preview::open_preview(item, app_icon, cx) {
            Ok(handle) => cx.global_mut::<Core>().preview = Some(handle),
            Err(e) => {
                log::warn!("preview failed: {e}");
                self.suppress_hide = false;
            }
        }
    }

    /// Called by the preview window when it closes itself.
    pub fn on_preview_closed(&mut self, paste: bool, window: &mut Window, cx: &mut Context<Self>) {
        cx.global_mut::<Core>().preview = None;
        self.suppress_hide = false;
        if paste {
            self.paste_selected(false, window, cx);
        } else {
            window.activate_window();
            window.focus(&self.focus_handle);
        }
    }

    // ---------- keyboard ----------

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // The shelf consumes every key. Marking the event handled stops gpui from re-dispatching
        // it through the text-input path (which would run this handler a second time).
        cx.stop_propagation();
        let ks = &event.keystroke;
        let m = ks.modifiers;
        let key = ks.key.as_str();

        if let Some(menu) = &self.menu {
            let entries = self.menu_entries(&menu.target, cx);
            match key {
                "escape" => self.menu = None,
                "down" | "up" => {
                    let n = entries.len();
                    if n > 0 {
                        let cur = menu.cursor;
                        let next = match (key == "down", cur) {
                            (true, None) => 0,
                            (true, Some(c)) => (c + 1) % n,
                            (false, None) => n - 1,
                            (false, Some(c)) => (c + n - 1) % n,
                        };
                        if let Some(m) = &mut self.menu {
                            m.cursor = Some(next);
                        }
                    }
                }
                "enter" | "space" => {
                    if let Some(action) = menu.cursor.and_then(|c| entries.get(c)).map(|e| e.action.clone()) {
                        self.run_menu_action(action, window, cx);
                    }
                }
                _ => {}
            }
            cx.notify();
            return;
        }

        if self.confirm_clear {
            match key {
                "enter" => self.clear_history(cx),
                "escape" => {
                    self.confirm_clear = false;
                    cx.notify();
                }
                _ => {}
            }
            return;
        }

        if !matches!(self.editing, Editing::None) {
            self.on_editing_key(ks, cx);
            return;
        }

        let typing = self.search_active || !self.query.is_empty();
        match key {
            "escape" => {
                if typing {
                    self.query.clear();
                    self.search_active = false;
                    self.refresh(cx);
                } else {
                    self.hide(window, cx);
                }
            }
            "left" => {
                if m.platform {
                    self.select(0, m.shift, false, window, cx);
                } else {
                    self.move_selection(-1, m.shift, window, cx);
                }
            }
            "right" => {
                if m.platform {
                    self.select(self.visible.len().saturating_sub(1), m.shift, false, window, cx);
                } else {
                    self.move_selection(1, m.shift, window, cx);
                }
            }
            "home" => self.select(0, m.shift, false, window, cx),
            "end" => self.select(self.visible.len().saturating_sub(1), m.shift, false, window, cx),
            "pageup" => {
                let p = self.page_size(window);
                self.move_selection(-p, m.shift, window, cx);
            }
            "pagedown" => {
                let p = self.page_size(window);
                self.move_selection(p, m.shift, window, cx);
            }
            "up" => {
                if m.platform {
                    self.select(0, m.shift, false, window, cx);
                } else {
                    self.cycle_tab(-1, cx);
                }
            }
            "down" => {
                if m.platform {
                    self.select(self.visible.len().saturating_sub(1), m.shift, false, window, cx);
                } else {
                    self.cycle_tab(1, cx);
                }
            }
            "tab" => self.cycle_tab(if m.shift { -1 } else { 1 }, cx),
            "enter" => {
                if m.alt {
                    self.copy_selected(window, cx);
                } else {
                    self.paste_selected(m.shift, window, cx);
                }
            }
            "space" => {
                if !self.query.trim().is_empty() && !m.platform {
                    self.query.push(' ');
                    self.refresh(cx);
                } else {
                    self.toggle_preview(window, cx);
                }
            }
            "backspace" => {
                if m.platform {
                    if typing {
                        self.query.clear();
                        self.search_active = false;
                        self.refresh(cx);
                    } else {
                        self.delete_selected(cx);
                    }
                } else if typing {
                    if self.query.is_empty() {
                        self.search_active = false;
                        cx.notify();
                    } else {
                        self.query.pop();
                        self.refresh(cx);
                    }
                }
            }
            "delete" => {
                if !typing {
                    self.delete_selected(cx);
                }
            }
            _ if m.platform => match key {
                "f" => {
                    self.search_active = true;
                    cx.notify();
                }
                "a" => {
                    if !self.visible.is_empty() {
                        self.multi = (0..self.visible.len()).collect();
                        cx.notify();
                    }
                }
                "c" => self.copy_selected(window, cx),
                "p" => {
                    if self.current_item().is_some() {
                        let off = -f32::from(self.scroll.offset().x);
                        let x = PAD_X + self.selected as f32 * STRIDE - off + 24.0;
                        let y = TOPBAR_H + STRIP_TOP + 60.0;
                        self.menu = Some(MenuState {
                            target: MenuTarget::Item(self.selected),
                            position: point(px(x.max(8.0)), px(y)),
                            cursor: Some(0),
                        });
                        cx.notify();
                    }
                }
                "o" => self.open_selected_link(window, cx),
                "n" => {
                    self.editing = Editing::NewPinboard(String::new());
                    cx.notify();
                }
                "," => crate::ui::prefs::open_prefs(cx),
                "w" => self.hide(window, cx),
                "q" => cx.quit(),
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => {
                    let n = key.parse::<usize>().unwrap_or(1) - 1;
                    let tabs = self.tabs(cx);
                    if let Some(t) = tabs.get(n) {
                        self.set_tab(*t, cx);
                    }
                }
                _ => {}
            },
            _ => {
                if m.control {
                    return;
                }
                if let Some(ch) = &ks.key_char {
                    if !ch.is_empty() && !ch.chars().any(|c| c.is_control()) {
                        self.query.push_str(ch);
                        self.search_active = true;
                        self.selected = 0;
                        self.anchor = 0;
                        self.multi.clear();
                        self.scroll.set_offset(point(px(0.), px(0.)));
                        self.refresh(cx);
                    }
                }
            }
        }
    }

    fn on_editing_key(&mut self, ks: &Keystroke, cx: &mut Context<Self>) {
        let key = ks.key.as_str();
        match key {
            "escape" => {
                self.editing = Editing::None;
                cx.notify();
            }
            "enter" => {
                let editing = std::mem::replace(&mut self.editing, Editing::None);
                match editing {
                    Editing::NewPinboard(name) => {
                        let name = if name.trim().is_empty() { "Pinboard".to_string() } else { name.trim().to_string() };
                        match cx.global_mut::<Core>().create_pinboard(&name) {
                            Ok(pb) => self.set_tab(Tab::Pinboard(pb.id), cx),
                            Err(e) => log::warn!("create pinboard: {e}"),
                        }
                    }
                    Editing::Rename(id, name) => {
                        if !name.trim().is_empty() {
                            let _ = cx.global_mut::<Core>().rename_pinboard(id, name.trim());
                        }
                    }
                    Editing::None => {}
                }
                cx.notify();
            }
            "backspace" => {
                match &mut self.editing {
                    Editing::NewPinboard(s) | Editing::Rename(_, s) => {
                        s.pop();
                    }
                    Editing::None => {}
                }
                cx.notify();
            }
            _ => {
                if ks.modifiers.platform || ks.modifiers.control {
                    return;
                }
                let ch = if key == "space" { Some(" ".to_string()) } else { ks.key_char.clone() };
                if let Some(ch) = ch {
                    if !ch.chars().any(|c| c.is_control()) {
                        match &mut self.editing {
                            Editing::NewPinboard(s) | Editing::Rename(_, s) => {
                                if s.chars().count() < 40 {
                                    s.push_str(&ch);
                                }
                            }
                            Editing::None => {}
                        }
                        cx.notify();
                    }
                }
            }
        }
    }

    // ---------- context menu ----------

    fn menu_entries(&self, target: &MenuTarget, cx: &App) -> Vec<MenuEntry> {
        let core = cx.global::<Core>();
        let mut out = Vec::new();
        let entry = |label: &str, action: MenuAction, sep: bool, shortcut: Option<&'static str>| MenuEntry {
            label: SharedString::from(label.to_string()),
            action,
            separator_before: sep,
            shortcut,
            dot: None,
        };
        match target {
            MenuTarget::Item(idx) => {
                let item = self.visible.get(*idx);
                let multi = self.selected_indexes().len() > 1;
                out.push(entry(if multi { "Paste All" } else { "Paste" }, MenuAction::Paste, false, Some("↩")));
                if item.map(|i| matches!(i.kind, ItemKind::RichText | ItemKind::Text | ItemKind::Link)).unwrap_or(false) {
                    out.push(entry("Paste as Plain Text", MenuAction::PastePlain, false, Some("⇧↩")));
                }
                out.push(entry("Copy", MenuAction::Copy, false, Some("⌘C")));
                out.push(entry("Preview", MenuAction::Preview, true, Some("Space")));
                if let Some(item) = item {
                    match item.kind {
                        ItemKind::Link => out.push(entry("Open Link", MenuAction::OpenLink, false, Some("⌘O"))),
                        ItemKind::File | ItemKind::Image => out.push(entry("Show in Finder", MenuAction::Reveal, false, None)),
                        _ => {}
                    }
                }
                let mut first = true;
                for pb in &core.pinboards {
                    if item.map(|i| core.is_pinned_in(pb.id, &i.hash)).unwrap_or(false) {
                        continue;
                    }
                    out.push(MenuEntry {
                        label: SharedString::from(format!("Add to {}", pb.name)),
                        action: MenuAction::AddToPinboard(pb.id),
                        separator_before: first,
                        shortcut: None,
                        dot: Some(hex_to_hsla(&pb.color)),
                    });
                    first = false;
                }
                out.push(entry("Add to New Pinboard…", MenuAction::NewPinboardWith, first, None));
                let pinned = item.map(|i| i.pinboard_id.is_some()).unwrap_or(false);
                let label = match (multi, pinned) {
                    (true, true) => "Remove Selected from Pinboard",
                    (true, false) => "Delete Selected",
                    (false, true) => "Remove from Pinboard",
                    (false, false) => "Delete",
                };
                out.push(entry(label, MenuAction::Delete, true, Some("⌘⌫")));
            }
            MenuTarget::Pinboard(id) => {
                out.push(entry("Rename Pinboard…", MenuAction::RenamePinboard(*id), false, None));
                out.push(entry("Change Color", MenuAction::RecolorPinboard(*id), false, None));
                out.push(entry("Delete Pinboard", MenuAction::DeletePinboard(*id), true, None));
            }
        }
        out
    }

    fn run_menu_action(&mut self, action: MenuAction, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        match action {
            MenuAction::Paste => self.paste_selected(false, window, cx),
            MenuAction::PastePlain => self.paste_selected(true, window, cx),
            MenuAction::Copy => self.copy_selected(window, cx),
            MenuAction::Preview => self.toggle_preview(window, cx),
            MenuAction::OpenLink => self.open_selected_link(window, cx),
            MenuAction::Reveal => self.reveal_selected(window, cx),
            MenuAction::AddToPinboard(id) => self.pin_selected_to(id, cx),
            MenuAction::NewPinboardWith => {
                let n = cx.global::<Core>().pinboards.len() + 1;
                match cx.global_mut::<Core>().create_pinboard(&format!("Pinboard {n}")) {
                    Ok(pb) => {
                        self.pin_selected_to(pb.id, cx);
                        self.editing = Editing::Rename(pb.id, pb.name.clone());
                    }
                    Err(e) => log::warn!("create pinboard: {e}"),
                }
            }
            MenuAction::Delete => self.delete_selected(cx),
            MenuAction::RenamePinboard(id) => {
                let name = cx.global::<Core>().pinboards.iter().find(|p| p.id == id).map(|p| p.name.clone()).unwrap_or_default();
                self.editing = Editing::Rename(id, name);
            }
            MenuAction::RecolorPinboard(id) => {
                let core = cx.global_mut::<Core>();
                if let Some(pb) = core.pinboards.iter().find(|p| p.id == id) {
                    let cur = PINBOARD_COLORS.iter().position(|c| c.eq_ignore_ascii_case(&pb.color)).unwrap_or(0);
                    let next = PINBOARD_COLORS[(cur + 1) % PINBOARD_COLORS.len()];
                    let _ = core.recolor_pinboard(id, next);
                }
            }
            MenuAction::DeletePinboard(id) => {
                let _ = cx.global_mut::<Core>().delete_pinboard(id);
                if self.tab == Tab::Pinboard(id) {
                    self.set_tab(Tab::History, cx);
                }
            }
        }
        cx.notify();
    }

    fn on_pill_drop(&mut self, pinboard_id: i64, drag: &DragItem, cx: &mut Context<Self>) {
        let mut ok = false;
        for &id in &drag.ids {
            let hash = cx.global::<Core>().db.get(id).ok().flatten().map(|i| i.hash);
            let already = hash.as_ref().map(|h| cx.global::<Core>().is_pinned_in(pinboard_id, h)).unwrap_or(true);
            if already {
                continue;
            }
            ok |= cx.global_mut::<Core>().pin_item(id, pinboard_id).is_ok();
        }
        if ok {
            let name = cx.global::<Core>().pinboards.iter().find(|p| p.id == pinboard_id).map(|p| p.name.clone()).unwrap_or_default();
            self.toast(format!("Added to {name}"), cx);
            if self.tab == Tab::Pinboard(pinboard_id) {
                self.refresh(cx);
            }
        }
    }

    // ---------- rendering ----------

    fn render_topbar(&mut self, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let pinboards = cx.global::<Core>().pinboards.clone();
        let paused = cx.global::<Core>().paused();
        let search_open = self.search_active || !self.query.is_empty();

        let search = div()
            .id("search")
            .h(px(30.))
            .rounded_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .px(px(9.))
            .cursor_pointer()
            .bg(if search_open { theme.field_bg } else { theme.pill_bg })
            .hover(|s| s.bg(theme.pill_hover))
            .when(search_open, |d| d.w(px(240.)))
            .on_click(cx.listener(|this, _, _, cx| {
                this.search_active = true;
                cx.notify();
            }))
            .child(svg().path(icon("search")).size(px(14.)).text_color(theme.text_secondary).flex_shrink_0())
            .when(search_open, |d| {
                d.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_row()
                        .items_center()
                        .text_size(px(13.))
                        .child(if self.query.is_empty() {
                            div().text_color(theme.text_tertiary).child("Search").into_any_element()
                        } else {
                            div().text_color(theme.text).truncate().child(SharedString::from(self.query.clone())).into_any_element()
                        })
                        .child(
                            div()
                                .w(px(1.5))
                                .h(px(15.))
                                .ml(px(1.))
                                .bg(theme.accent)
                                .with_animation(
                                    "caret",
                                    Animation::new(Duration::from_millis(1000)).repeat().with_easing(pulsating_between(0.1, 1.0)),
                                    |d, t| d.opacity(t),
                                ),
                        ),
                )
                .child(
                    div()
                        .id("search-clear")
                        .size(px(16.))
                        .rounded_full()
                        .bg(theme.text_tertiary)
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.query.clear();
                            this.search_active = false;
                            this.refresh(cx);
                            cx.stop_propagation();
                        }))
                        .child(svg().path(icon("close")).size(px(8.)).text_color(theme.card_bg)),
                )
            });

        let mut pills = div().flex().flex_row().items_center().gap(px(4.)).min_w_0().overflow_hidden();
        pills = pills.child(self.render_pill(0, Tab::History, "Clipboard", None, theme, cx));
        for (i, pb) in pinboards.iter().enumerate() {
            let dot = Some(hex_to_hsla(&pb.color));
            let editing_name = match &self.editing {
                Editing::Rename(id, name) if *id == pb.id => Some(name.clone()),
                _ => None,
            };
            if let Some(name) = editing_name {
                pills = pills.child(self.render_edit_field(name, theme));
            } else {
                pills = pills.child(self.render_pill(i + 1, Tab::Pinboard(pb.id), &pb.name, dot, theme, cx));
            }
        }
        if let Editing::NewPinboard(name) = &self.editing {
            pills = pills.child(self.render_edit_field(name.clone(), theme));
        }
        pills = pills.child(
            icon_button("new-pinboard", "plus", 13., theme).on_click(cx.listener(|this, _, _, cx| {
                this.editing = Editing::NewPinboard(String::new());
                cx.notify();
            })),
        );

        let right = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.))
            .flex_shrink_0()
            .when(paused, |d| {
                d.child(
                    div()
                        .id("paused")
                        .h(px(26.))
                        .px(px(10.))
                        .rounded_full()
                        .bg(theme.field_bg)
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(11.5))
                        .text_color(theme.text_secondary)
                        .cursor_pointer()
                        .on_click(cx.listener(|_, _, _, cx| {
                            cx.global::<Core>().set_paused(false);
                            cx.notify();
                        }))
                        .child(svg().path(icon("pause")).size(px(11.)).text_color(theme.text_secondary))
                        .child("Paused"),
                )
            })
            .child(icon_button("settings", "gear", 16., theme).on_click(|_, _, cx| crate::ui::prefs::open_prefs(cx)));

        div()
            .h(px(TOPBAR_H))
            .flex_shrink_0()
            .px(px(PAD_X - 6.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.))
            .child(search)
            .child(pills)
            .child(div().flex_1())
            .child(right)
    }

    fn render_edit_field(&self, value: String, theme: &Theme) -> Div {
        div()
            .h(px(28.))
            .min_w(px(120.))
            .px(px(10.))
            .rounded_full()
            .bg(theme.field_bg)
            .border_1()
            .border_color(theme.accent)
            .flex()
            .items_center()
            .text_size(px(12.5))
            .child(if value.is_empty() {
                div().text_color(theme.text_tertiary).child("Pinboard name").into_any_element()
            } else {
                div().text_color(theme.text).child(SharedString::from(value)).into_any_element()
            })
            .child(div().w(px(1.5)).h(px(14.)).ml(px(1.)).bg(theme.accent))
    }

    fn render_pill(&self, ix: usize, tab: Tab, name: &str, dot: Option<Hsla>, theme: &Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = self.tab == tab;
        let accent = theme.accent;
        let pill_hover = theme.pill_hover;
        let mut pill = div()
            .id(("pill", ix))
            .h(px(28.))
            .px(px(11.))
            .rounded_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .flex_shrink_0()
            .cursor_pointer()
            .text_size(px(12.5))
            .font_weight(if selected { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM })
            .text_color(if selected { theme.text } else { theme.text_secondary })
            .bg(if selected { theme.pill_selected } else { theme.pill_bg })
            .hover(move |s| s.bg(pill_hover))
            .on_click(cx.listener(move |this, _, _, cx| this.set_tab(tab, cx)))
            .child(match dot {
                Some(c) => div().size(px(9.)).rounded_full().bg(c).flex_shrink_0().into_any_element(),
                None => svg()
                    .path(icon("clipboard"))
                    .size(px(13.))
                    .text_color(if selected { theme.text } else { theme.text_secondary })
                    .flex_shrink_0()
                    .into_any_element(),
            })
            .child(SharedString::from(name.to_string()));
        if let Tab::Pinboard(id) = tab {
            pill = pill
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        this.menu = Some(MenuState { target: MenuTarget::Pinboard(id), position: ev.position, cursor: None });
                        cx.notify();
                    }),
                )
                .drag_over::<DragItem>(move |style, _, _, _| style.bg(accent.opacity(0.35)))
                .on_drop(cx.listener(move |this, drag: &DragItem, _, cx| this.on_pill_drop(id, drag, cx)));
        }
        pill
    }

    fn render_strip(&mut self, theme: &Theme, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        if self.visible.is_empty() {
            let (title, subtitle) = if !self.query.trim().is_empty() {
                (format!("No results for “{}”", self.query.trim()), "Try a different search term".to_string())
            } else if let Tab::Pinboard(_) = self.tab {
                ("This pinboard is empty".to_string(), "Drag items here or use “Add to Pinboard” from the context menu".to_string())
            } else {
                ("Your clipboard history is empty".to_string(), "Copy something and it will show up here".to_string())
            };
            return div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).text_color(theme.text_secondary).child(SharedString::from(title)))
                .child(div().text_size(px(12.5)).text_color(theme.text_tertiary).child(SharedString::from(subtitle)))
                .into_any_element();
        }

        let total = self.visible.len();
        let view_w = self.view_width(window);
        let off = (-f32::from(self.scroll.offset().x)).max(0.0);
        let first = ((off / STRIDE).floor() as usize).min(total.saturating_sub(1));
        let count = ((view_w / STRIDE).ceil() as usize) + 2;
        let last = (first + count).min(total);
        let left_w = first as f32 * STRIDE;
        let right_w = (total - last) as f32 * STRIDE;
        let now = self.now;
        let hovered = self.hovered;
        let sel = self.selected;
        let multi = self.multi.clone();
        let selected_ids = self.selected_ids();

        let mut icons: Vec<AppIcon> = Vec::with_capacity(last - first);
        let mut pending: Vec<bool> = Vec::with_capacity(last - first);
        {
            let core = cx.global_mut::<Core>();
            for item in &self.visible[first..last] {
                icons.push(core.icon_for(&item.app_bundle));
                pending.push(item.kind == ItemKind::Link && item.link_image_path.is_none() && core.is_link_pending(item.id));
            }
        }

        let mut strip = div()
            .id("strip")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_row()
            .items_start()
            .pt(px(STRIP_TOP))
            .px(px(PAD_X))
            .gap(px(CARD_GAP))
            .overflow_x_scroll()
            .track_scroll(&self.scroll);

        if left_w > 0.0 {
            strip = strip.child(div().w(px(left_w - CARD_GAP)).h(px(1.)).flex_shrink_0());
        }
        for (i, item) in self.visible[first..last].iter().enumerate() {
            let idx = first + i;
            let is_sel = idx == sel || multi.contains(&idx);
            let item_arc = item.clone();
            let tint = icons[i].tint;
            let label: SharedString = format!("{} · {}", item.kind.label(), crate::ui::card::caption(item)).into();
            let card = render_card(idx, item, &icons[i], theme, is_sel, hovered == Some(idx), now, pending[i])
                .on_hover(cx.listener(move |this, is_hover: &bool, _, cx| {
                    let next = if *is_hover { Some(idx) } else if this.hovered == Some(idx) { None } else { this.hovered };
                    if next != this.hovered {
                        this.hovered = next;
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                    let m = ev.modifiers();
                    if ev.click_count() >= 2 {
                        this.select(idx, false, false, window, cx);
                        this.paste_selected(false, window, cx);
                    } else {
                        this.select(idx, m.shift, m.platform, window, cx);
                    }
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                        if !this.multi.contains(&idx) {
                            this.select(idx, false, false, window, cx);
                        } else {
                            this.selected = idx;
                        }
                        this.menu = Some(MenuState { target: MenuTarget::Item(idx), position: ev.position, cursor: None });
                        cx.notify();
                    }),
                )
                .on_drag(
                    DragItem { ids: if is_sel && selected_ids.len() > 1 { selected_ids.clone() } else { vec![item_arc.id] } },
                    move |_, _, _, cx| {
                        cx.new(|_| DragGhost { label: label.clone(), tint })
                    },
                );
            strip = strip.child(card);
        }
        if right_w > 0.0 {
            strip = strip.child(div().w(px(right_w - CARD_GAP)).h(px(1.)).flex_shrink_0());
        }
        strip.into_any_element()
    }

    fn render_menu(&self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.menu.as_ref()?;
        let cursor = menu.cursor;
        let entries = self.menu_entries(&menu.target, cx);
        let mut list = div()
            .id("context-menu")
            .occlude()
            .min_w(px(210.))
            .p(px(5.))
            .rounded(px(10.))
            .bg(theme.menu_bg)
            .border_1()
            .border_color(theme.menu_border)
            .shadow_lg()
            .flex()
            .flex_col()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.menu = None;
                cx.notify();
            }));
        for (i, e) in entries.into_iter().enumerate() {
            if e.separator_before && i > 0 {
                list = list.child(div().h(px(1.)).my(px(4.)).mx(px(8.)).bg(theme.separator));
            }
            let action = e.action.clone();
            let hover_bg = theme.menu_hover;
            list = list.child(
                div()
                    .id(("menu-item", i))
                    .h(px(26.))
                    .px(px(9.))
                    .rounded(px(6.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(13.))
                    .text_color(theme.text)
                    .cursor_pointer()
                    .when(cursor == Some(i), |d| d.bg(hover_bg).text_color(white()))
                    .hover(move |s| s.bg(hover_bg).text_color(white()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.run_menu_action(action.clone(), window, cx);
                    }))
                    .when_some(e.dot, |d, c| d.child(div().size(px(9.)).rounded_full().bg(c).flex_shrink_0()))
                    .child(div().flex_1().child(e.label))
                    .when_some(e.shortcut, |d, s| d.child(div().text_size(px(11.5)).opacity(0.6).child(s))),
            );
        }
        Some(
            deferred(
                anchored()
                    .position(menu.position)
                    .anchor(Corner::TopLeft)
                    .snap_to_window_with_margin(px(8.))
                    .child(list),
            )
            .with_priority(10)
            .into_any_element(),
        )
    }

    fn render_confirm_clear(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        deferred(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(hsla(0., 0., 0., 0.25))
                .child(
                    div()
                        .id("confirm-clear")
                        .occlude()
                        .w(px(360.))
                        .p(px(18.))
                        .rounded(px(14.))
                        .bg(theme.menu_bg)
                        .border_1()
                        .border_color(theme.menu_border)
                        .shadow_lg()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).text_color(theme.text).child("Clear clipboard history?"))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(theme.text_secondary)
                                .child("All items in Clipboard will be removed. Pinboards are kept. This can't be undone."),
                        )
                        .child(
                            div()
                                .mt(px(10.))
                                .flex()
                                .flex_row()
                                .justify_end()
                                .gap(px(8.))
                                .child(crate::ui::widgets::button("cancel-clear", "Cancel", theme, false).on_click(cx.listener(|this, _, _, cx| {
                                    this.confirm_clear = false;
                                    cx.notify();
                                })))
                                .child(crate::ui::widgets::button("do-clear", "Clear History", theme, true).on_click(cx.listener(|this, _, _, cx| this.clear_history(cx)))),
                        ),
                ),
        )
        .with_priority(20)
        .into_any_element()
    }
}

impl Focusable for Shelf {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Shelf {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pref = cx.global::<Core>().settings.read().unwrap().theme;
        let theme = Theme::for_window(window, pref);
        let topbar = self.render_topbar(&theme, cx);
        let strip = self.render_strip(&theme, window, cx);
        let menu = self.render_menu(&theme, cx);
        let toast = self.toast.clone();
        let confirm = if self.confirm_clear { Some(self.render_confirm_clear(&theme, cx)) } else { None };

        let panel = div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.shelf_bg)
            .border_t_1()
            .border_color(theme.shelf_border)
            .child(topbar)
            .child(strip)
            .with_animation(
                ("shelf-slide", self.show_gen),
                Animation::new(Duration::from_millis(230)).with_easing(ease_out_quint()),
                |d, t| d.mt(px(SHELF_H * (1.0 - t))),
            );

        div()
            .id("shelf-root")
            .size_full()
            .relative()
            .overflow_hidden()
            .track_focus(&self.focus_handle)
            .key_context("Shelf")
            .on_key_down(cx.listener(Self::on_key_down))
            .text_color(theme.text)
            .child(panel)
            .children(menu)
            .children(confirm)
            .when_some(toast, |d, msg| {
                d.child(
                    div()
                        .absolute()
                        .bottom(px(14.))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .px(px(14.))
                                .py(px(7.))
                                .rounded_full()
                                .bg(if theme.dark { hsla(0., 0., 0.1, 0.92) } else { hsla(0., 0., 0.15, 0.9) })
                                .text_size(px(12.5))
                                .text_color(white())
                                .shadow_md()
                                .child(msg),
                        ),
                )
            })
    }
}
