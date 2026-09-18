# Paste (Rust, native macOS)

A native clipboard manager for Apple Silicon Macs, modeled on [Paste](https://pasteapp.io/):
a shelf slides up from the bottom of the screen with everything you copied as cards,
organised into pinboards, searchable and pasteable with the keyboard.

Written in Rust. UI is rendered with [GPUI](https://gpui.rs) (Metal), macOS integration
uses `objc2` bindings to AppKit directly. No Electron, no web view.

## Features

- **Clipboard history** – text, rich text (RTF/HTML), links, colors, images and files, with the
  source app's icon and tint on every card. Duplicates are merged and moved to the top.
- **Shelf UI** – `⌘⇧V` slides the shelf up over the Dock on the screen under your cursor.
  Vibrancy background, animated entrance, light/dark themes, keyboard-first.
- **Pinboards** – keep snippets, links, colors and templates forever. Drag a card onto a
  pinboard tab or use the context menu. Rename, recolor and delete from the tab's context menu.
- **Search** – just start typing. Full-text search (SQLite FTS5) across the whole history.
- **Quick Look** – `Space` opens a large preview of the selected item.
- **Paste back** – `↩` pastes into the app you came from, `⇧↩` pastes as plain text,
  `⌥↩` / `⌘C` copies without pasting. Select several cards (`⇧`/`⌘` click or `⇧←→`) to paste them all.
- **Rules** – ignore chosen apps (password managers are pre-filled) and content flagged as
  confidential/transient by other apps.
- **Menu bar item** with pause, clear history, settings and quit. Launch at login option.
- **History retention** – forever, or 1 year / month / week / day.
- Everything stays on your Mac (`~/Library/Application Support/Paste`). Link titles/favicons are
  fetched from the page itself and can be turned off.

## Keyboard

| Action | Keys |
| --- | --- |
| Show / hide Paste | `⌘⇧V` (configurable in Settings → Shortcuts) |
| Move between cards | `←` `→`, `Home`/`End`, `PageUp`/`PageDown` |
| Extend / toggle selection | `⇧←` `⇧→`, `⇧Click`, `⌘Click`, `⌘A` |
| Paste / paste plain / copy only | `↩` / `⇧↩` / `⌥↩` or `⌘C` |
| Quick Look | `Space` |
| Search | type anything, `⌘F`; `Esc` clears |
| Switch pinboard | `⇥`, `⇧⇥`, `↑` `↓`, `⌘1…9` |
| New pinboard / add to pinboard | `⌘N` / `⌘P` or drag onto a tab |
| Open link | `⌘O` |
| Delete | `⌘⌫` or `⌦` |
| Settings / close / quit | `⌘,` / `Esc` `⌘W` / `⌘Q` |

## Build

Requirements: macOS 13+, Apple Silicon, Rust 1.85+ and Xcode Command Line Tools (a full
Xcode install is not needed; Metal shaders are compiled at runtime).

```sh
cargo run --release          # run directly (menu bar app, no Dock icon)
./packaging/build-app.sh     # build dist/Paste.app (ad-hoc signed)
cp -R dist/Paste.app /Applications/
```

On first paste macOS asks for **Accessibility** access (System Settings → Privacy & Security →
Accessibility). Without it Paste still copies the item to the clipboard, but cannot send the
`⌘V` keystroke into the target app. Grant it to the app you actually run (`Paste.app`, or your
terminal when using `cargo run`).

## Tests

```sh
cargo test
```

## Scripting / debugging

Paste listens on a local Unix socket at `~/Library/Application Support/Paste/control.sock`.
One command per line: `show`, `hide`, `toggle`, `prefs`, `preview`, `dump`, `quit`,
`key <keystroke>` (e.g. `key cmd-1`), `shot <prefix>` (saves PNGs of the open windows).

```sh
echo toggle | nc -U ~/Library/Application\ Support/Paste/control.sock
```

`kill -USR1 <pid>` toggles the shelf and `kill -USR2 <pid>` opens Settings.

## Layout

```
src/
  main.rs          app bootstrap, event loop, control socket
  core.rs          shared state (history, pinboards, icons, windows)
  clipboard.rs     NSPasteboard monitor thread → items
  db.rs            SQLite storage + FTS5 search
  link_preview.rs  page title / favicon fetch
  hotkey.rs, tray.rs, settings.rs, model.rs, util.rs
  mac/             objc2 AppKit glue: pasteboard, workspace, paste keystroke, screens, windows, login item
  ui/              GPUI views: shelf (main panel), card, preview (Quick Look), prefs (Settings), widgets, theme
packaging/         Info.plist + build-app.sh
```
