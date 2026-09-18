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

## Install from source

Paste is not distributed as a signed download; you build it yourself in a few minutes.

**Requirements**

- A Mac with Apple Silicon (M1 or later) running macOS 13 or newer.
- Xcode Command Line Tools. A full Xcode install is not needed (Metal shaders compile at runtime).
- Rust 1.85 or newer.

**1. Install the tools**

```sh
# Xcode Command Line Tools (skip if `git` already works in Terminal)
xcode-select --install

# Rust toolchain via rustup, then reload your shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustc --version   # should print 1.85.0 or newer
```

**2. Get the code**

```sh
git clone https://github.com/nikolaevK/paste-rs.git
cd paste-rs
```

**3. Build the app bundle**

```sh
./packaging/build-app.sh
```

The first build downloads and compiles all dependencies and takes a few minutes; later builds
take seconds. The script produces `dist/Paste.app`, ad-hoc signed for your machine.

**4. Install and run**

```sh
cp -R dist/Paste.app /Applications/
open /Applications/Paste.app
```

Paste runs as a menu bar app with no Dock icon. Press `⌘⇧V` to open the shelf, or click the
clipboard icon in the menu bar.

**5. Grant Accessibility access**

On your first paste macOS asks for **Accessibility** access. Grant it in
System Settings → Privacy & Security → Accessibility (toggle on Paste). Without it Paste still
copies the selected item to the clipboard but cannot send `⌘V` into the target app.

If you run Paste from the terminal instead (`cargo run --release`), grant the permission to your
terminal app rather than to Paste.

**Updating**

```sh
cd paste-rs
git pull
./packaging/build-app.sh
cp -R dist/Paste.app /Applications/
```

**Uninstalling**

```sh
rm -rf /Applications/Paste.app
rm -rf ~/Library/Application\ Support/Paste          # history, images, settings
rm -f ~/Library/LaunchAgents/io.paste.rs.plist        # only if "Launch at login" was enabled
```

**Troubleshooting**

- *"Paste.app is damaged" or Gatekeeper refuses to open it* – the bundle is only ad-hoc signed.
  Run `xattr -dr com.apple.quarantine /Applications/Paste.app` once, or build it locally as above.
- *Nothing pastes, the item is only copied* – Accessibility access is missing (see step 5).
- *`⌘⇧V` does nothing* – another app owns that shortcut. Change it in Settings → Shortcuts
  (menu bar icon → Settings…).
- *Build fails with a Rust version error* – run `rustup update`.

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
