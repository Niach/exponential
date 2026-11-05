# eDE

The Exponential Desktop Environment (#NEW-10): a Wayland compositor plus a
shell drawn in genUI. This is its own Cargo workspace; it depends on the shared
libraries, never the other way round.

| Crate | Role |
|---|---|
| `ede-session` | What the login screen starts: the session environment (`XDG_CURRENT_DESKTOP=eDE`, Wayland hints for Firefox/Qt/Electron), `ede-compositor` on the seat, then `ede-session.target` (XDG autostart via systemd's generator, polkit agent, gnome-keyring). Restarts a crashed compositor; three quick crashes in a row (also across logins) bring it back in safe mode. |
| `ede-protocol` | The control plane (#NEW-104): wire types, framing and a blocking `Client` for the compositor's socket, on Core's `Capability`/`Grant`/`Failure`/`Envelope`. Computer Use, eos-agent and the shell link it. |
| `ede-ui` | The shell's genUI on disk (#EDE-9): documents in `~/.config/ede/ui` (factory copies in `/usr/share/ede/ui`), validated before they apply, hot reloaded, the last good version kept, every edit a git commit, undo = revert. The shell and ede-tools link it; `ede-ui` is its CLI. |
| `ede-ctl` | The command line over `ede-protocol` (#EDE-7): `windows`, `outputs`, `capture --png` (the shots capture and the agent harness in the VM), `key`, `pointer`. |
| `ede-compositor` | smithay compositor: floating windows with magnetic edges and edge tiling, fullscreen, xdg-shell, wlr-layer-shell, Xwayland (X11 apps and games), linux-dmabuf, drm-syncobj explicit sync (udev), ext-session-lock, idle notify/inhibit, input methods (input-method-v2, text-input-v3, virtual keyboard), protected surfaces, damage-tracked rendering, `~/.config/ede/compositor.toml`. Backends: `winit` (nested, development), `udev` (the real session: DRM/KMS, libinput, hotplug; feature `udev`) and `headless` (CI). |

## The shell (`shell/`, a second workspace)

`apps/ede/shell` is its own Cargo workspace (#EDE-10): `ede-shell` draws the
shell from the genUI documents `ede-ui` serves (below) with gpui + eui-gpui
at the SDK's pins (the painter README's "gpui revisions", gated by
`check-gpui-revs.ts`), Wayland backend only. The compositor workspace never
links gpui, and the shell is a separate process under the supervisor, so a
shell crash never takes the session down. Every document with a layer
`surface` header is a layer-shell surface (`WindowKind::LayerShell`); a
popup document opens from its anchor component in its parent as an
`xdg_popup` of that surface (so a menu can leave the panel; eui-gpui paints
its own overlays inside their window). A change on disk redraws the surface
and the shell prints the document's frame line, which the supervisor reads.
Data every document may bind: `/time` (local), `/windows` (the open
windows over ede-protocol); actions: `focus {window}`, `launch {command}`.
Icons resolve by concept from `packages/icons` (the IDE's Lucide files and
fonts are embedded). Factory documents live in `apps/ede/ui/*.json`:
`panel.json` (top, 40 px exclusive zone, the launcher button, the taskbar,
the clock) and `menu.json` (the panel's popup).

```bash
apps/ede/scripts/ede-dev.sh                   # nested in the desktop around it: compositor + supervisor + shell
apps/ede/scripts/ede-dev.sh --headless 1280x720
WAYLAND_DISPLAY=wayland-1 target/debug/ede-ctl capture --png /tmp/panel.png --expect-drawn
WAYLAND_DISPLAY=wayland-1 target/debug/ede-ctl pointer 40 20 --click   # the menu
```

The M0 demo (#EDE-1), in the `ubuntu-wayland` VM: edit the scratch
directory's `panel.json` and the panel redraws within the watcher's
debounce; break it and `ede-ui check` says why while the last good version
stays on screen; `git -C <dir> log` shows `Edit panel.json` commits after
the factory ones. `ede-shell --windowed` opens normal windows instead
(macOS, any desktop); `--popup` opens every popup document right away and
`--exit-after <ms>` quits, for the headless check.

## Run it nested (Cinnamon/X11)

```bash
cd apps/ede
cargo run -- -- gnome-calculator     # anything after `--` starts inside eDE
cargo run -- -- xterm                # X11 apps run through Xwayland
WAYLAND_DISPLAY=wayland-1 xed        # or point a client at the socket it logs
```

Driving it with `xdotool` on the live desktop sends keys to whatever window
is focused there, not necessarily eDE: script input against a private
`Xvfb :77` (Mesa EGL: `__EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json`).

## Doctor

```bash
cargo run -- doctor
```

Checks the GPU render nodes, Xwayland (the binary the compositor will run
and its version: 24.1+ has explicit sync; older versions flicker with X11
games on NVIDIA), and on NVIDIA that
`nvidia-drm.modeset` is on (the parameter, the kernel command line, or the
driver default from 560).

The compositor opens one window on the running X11 (or Wayland) session and
serves `$XDG_RUNTIME_DIR/wayland-N`. Close the window to quit.
`--headless 1920x1080` runs without a window (pixman, no GPU).

Bindings: Alt+drag moves a window, dropping it on a side edge tiles it to a
half (corners = quarter, top = maximize), Super+Left/Right tile, Super+Up
maximizes, Super+Down restores, Alt+F4 closes, Super+L locks, Super+Shift+E quits
(logs out of the session), Ctrl+Alt+F1…F12 switch VTs (udev).

## Run it as the session (a TTY)

```bash
cargo build --release --features udev
# switch to a free TTY (Ctrl+Alt+F3), log in, then:
target/release/ede-compositor -- gnome-calculator
```

With neither `DISPLAY` nor `WAYLAND_DISPLAY` set it takes the seat through
libseat (logind or seatd): every connected monitor becomes an output, left
to right; plugging one in or out re-lays them and moves stranded windows to
the first. `EDE_DRM_DEVICE=/dev/dri/cardN` picks the GPU. Started inside a
running X11/Wayland session it nests instead; never pass `--udev` there,
the other display server owns the GPU. The login screen entry is
`ede-session` (below).

## Install as a login session (Mint/Ubuntu)

```bash
apps/ede/packaging/build-deb.sh          # → apps/ede/target-deb/ede_<version>_amd64.deb
sudo apt install ./apps/ede/target-deb/ede_*_amd64.deb
```

The .deb bundles Xwayland 24.1 at `/usr/lib/ede/bin/Xwayland`, because
Mint 22 / Ubuntu noble ships 23.2.6, which has no explicit sync.
`packaging/xwayland/build.sh` builds it: a pinned, sha256-checked tarball,
built with meson against noble's own libraries. It runs in an
`ubuntu:24.04` container when the host has no meson, and the result is
reused until the script changes. The compositor puts `/usr/lib/ede/bin`
first on Xwayland's `PATH`. `--no-xwayland` skips the bundled build (quick
local builds) and depends on the distro's `xwayland`. CI: `build-ede.yml`
(`ede-v*` tags, by hand, PRs that touch `packaging/`).

Check on the real machine after installing: `ede-compositor doctor` shows
`ok Xwayland /usr/lib/ede/bin/Xwayland 24.1.x, explicit sync`. In the
session, an X11 game (Steam/Proton) on NVIDIA no longer flickers, and
`pgrep -a Xwayland` shows `/usr/lib/ede/bin/Xwayland`.

The package carries the compositor, `ede-session`, `ede-ui` and `ede-shell`
in `/usr/bin`, the factory documents in `/usr/share/ede/ui`, and the
session's user units (#EDE-2): once the compositor is up, `ede-session
--ready` hands `WAYLAND_DISPLAY` to systemd and starts `ede-session.target`,
which wants `ede-shell.service` = `ede-ui supervise -- ede-shell`, so a
login gets the panel under the supervisor (a crashed shell restarts, a bad
edit rolls back; `systemctl --user status ede-shell` and `journalctl --user
-u ede-shell` show it). `--dev` packages the dev profile's binaries for a
quick check in a VM.

Log out; LightDM's session menu now has **Exponential** and **Exponential
(safe mode)** next to Cinnamon. Safe mode starts with the stock settings
(`--safe-mode`: the config file is not read; `EDE_SAFE_MODE=1` reaches the
shell's unit, and `ede-ui` then supervises a scratch copy of the factory
documents, leaving `~/.config/ede/ui` untouched). Super+Shift+E logs out;
Ctrl+Alt+F1…F12 switch VTs. Logs: `journalctl --user -b` and
`~/.xsession-errors`. Remove with `sudo apt remove ede`; Cinnamon is
untouched.

## ede-protocol

`$XDG_RUNTIME_DIR/ede/<wayland-N>.sock` (directory 0700): frames of a
little-endian `u32` length plus JSON (`{"id": 1, "op": "list_windows"}`),
the pixels of a capture as the frame after its reply. `hello` with a token
first; each op needs the Core capability it names:

| Op | Capability |
|---|---|
| `list_windows`, `list_outputs`, `subscribe` (events: `window.opened/closed/focused`, `session.locked/unlocked`, `config.reloaded`) | any grant |
| `focus_window`, `close_window`, `move_window` | `ui` |
| `capture` (an output: protected windows black, locked = black; a window: protected ones refused) | `capture` |
| `input` (key, pointer motion, button; refused on protected surfaces and the lock) | `input:person` |
| `reload_config`, `lock`, `mint_grant`, `revoke_grant` | `system` |

The compositor writes the root token (every capability) to
`$XDG_RUNTIME_DIR/ede/<wayland-N>.token` (0600) for the shell; `mint_grant`
hands narrower, expiring grants to agents (never more than the minting grant
holds, never outliving it). Any process of the same user can read the token
file, so the boundary is between the shell and the agents it mints for (an
agent sandbox does not see `$XDG_RUNTIME_DIR/ede`). Agent spaces and window
lending come with their own issues; captures are drawn in software for now.

```rust
let mut ede = ede_protocol::Client::connect_env()?;   // $WAYLAND_DISPLAY + the token file (or $EDE_TOKEN)
for w in ede.windows()? { println!("{} {:?}", w.id, w.title); }
```

## Config

`$XDG_CONFIG_HOME/ede/compositor.toml`, every key optional; a broken file is
logged and ignored.

```toml
background = "#0a0a0a"
xwayland = true       # or --no-xwayland

[keyboard]
layout = "de"
repeat_delay = 400
repeat_rate = 30

[windows]
snap_threshold = 12   # px; 0 = off
edge_tiling = true

[lock]
command = ["swaylock", "-f", "-c", "000000"]   # any ext-session-lock client; empty = no locking
on_suspend = true     # lock before suspend and on `loginctl lock-session` (udev)

[idle]
lock_after = 600      # seconds without input; 0 = never

[protected]
app_ids = ["org.example.Bank"]   # on top of the built-in list
```

## Lock, idle, input methods, protected surfaces

The lock screen is a client of `ext-session-lock` (the genUI shell's, later;
`swaylock` today). From the lock request on, every output draws only that
client's lock surface (black until it draws), only it gets keys and clicks,
and bindings are off except Ctrl+Alt+F1…F12. If the lock client crashes the
session stays locked (black) until a new lock client takes over; a second
client can neither lock over it nor unlock it. Super+L, `[idle] lock_after`
(held off by idle inhibitors such as a playing video), `loginctl
lock-session` and suspend run `[lock] command`; before suspend a logind delay
inhibitor waits up to 2 s for the lock to show. `ext-idle-notify` is there for
the shell's screen blanking.

Protected surfaces are never captured, lent or driven: the lock screen,
polkit and keyring prompts, pinentry, ssh askpass and password managers
(`ede_compositor::protected::BUILTIN`, matched by app id, WM_CLASS or the
process's executable), plus `[protected] app_ids`. Captures draw them black
(`backend::capture_elements`; a locked session is black altogether), agent
input to them is refused, a virtual keyboard cannot type into them, and an
input method loses its keyboard grab and stays off while one has focus.

Try it on real hardware (Danny; never from a session another display server
owns): `sudo apt install swaylock`, put the `[lock]` block above in the
config, log into **Exponential**, then check: Super+L locks and the password
unlocks; `loginctl lock-session` locks; `systemctl suspend` and wake shows
the lock screen, never the desktop; `pkill -9 swaylock` while locked leaves a
black screen, and `swaylock` from a VT (`WAYLAND_DISPLAY=wayland-1
XDG_RUNTIME_DIR=/run/user/$UID swaylock`) takes over; a video in Firefox
holds off the idle lock; fcitx5 or ibus types into a GTK app but not into a
polkit prompt (`pkexec true`).

## genUI on disk (ede-ui)

`$XDG_CONFIG_HOME/ede/ui/` (default `~/.config/ede/ui/`; `$EDE_UI_DIR` wins)
holds the shell: one genUI surface per `<name>.json` (lowercase, digits, `-`,
`_`) and the look in `theme.json` (the theme format of the SDK; `extends` may
name a built-in theme; none = `exponential`). The factory copies live in
`/usr/share/ede/ui` (`$EDE_FACTORY_DIR`; `apps/ede/ui` in this repo): a fresh
account gets them as the first commits, and a rollback always has somewhere
to go. The contract is `packages/ede` (#EDE-8): its fixtures are the spec and
`crates/ede-ui/tests/contract.rs` replays them.

```json
{
  "surface": {"layer": "top", "anchor": ["top", "left", "right"], "size": {"height": 40}, "exclusiveZone": 40},
  "root": {"id": "panel", "component": "Box", "style": {"display": "flex"},
           "children": [{"id": "clock", "component": "Text", "props": {"text": {"path": "/time"}}}]},
  "data": {"time": "12:00"}
}
```

`surface` says where the shell puts it: a layer surface (`layer`, `anchor`,
`size`, `exclusiveZone`, `margin`, `keyboard`) or `popup` (a popup of another
document under one of its components, drawn as an `xdg_popup` so it can leave
the parent's box); `functions` lists the host functions the document may
call. `root` is the nested authoring form; `components` (A2UI's flat
`updateComponents` list, from the component `root`) works too; `catalogId`
defaults to the core. Tier C (a document grants no privileges): no password
inputs, no `functionCall` outside `surface.functions`, ids once; then
eui-core's reducer validates every component, prop, style and token. One
issue refuses the whole file and the last good version stays on screen
(after a restart, the last committed one). The directory is a git repository
(the git CLI; created on first open): every edit that applies is a commit, an
edit through the API with its own message, a hand edit as `Edit <files>` once
it validates; refused files are never committed. Undo reverts the newest
commit as a new commit, so an undo is undoable too.

```bash
cargo run -p ede-ui -- check                    # what applies, what is refused and why
cargo run -p ede-ui -- set panel panel.json     # validate, write, commit
cargo run -p ede-ui -- undo                     # revert the newest edit
cargo run -p ede-ui -- discard panel            # drop a broken hand edit
cargo run -p ede-ui -- log
cargo run -p ede-ui -- watch                    # hot reload, one JSON change per line
cargo run -p ede-ui -- supervise -- ede-shell   # run the shell under the watchdog
```

**The supervisor** (`ede-ui supervise -- <shell command>`) owns the history
and runs the apply machine of `packages/ede/fixtures/apply.json`: it starts
the shell with `EDE_UI_DIR`, commits hand edits, refuses broken ones, and
expects the shell to print `{"frame": {"name": "panel", "digest": "…"}}`
(`ede-ui digest FILE`, the content digest) on stdout once it drew a document.
No frame within 2 s of an edit = the edit is reverted (`Undo: Edit
panel.json, rolled back`); the shell dying counts as a crash, it restarts
after 500 ms, and three crashes after an edit revert it, three more put the
factory copy back (`Factory panel`). The shell itself opens the directory
read-only (`Runtime::open_readonly`): it validates and reloads but never
writes history.

```rust
let (ui, _) = ede_ui::Runtime::open_readonly(ede_ui::default_dir())?;
let ui = std::sync::Arc::new(std::sync::Mutex::new(ui));
let _w = ede_ui::watch(ui.clone(), |changes| for c in changes { println!("{}", c.kind()) })?;
// ui.lock().document("panel") → .surface (where it goes) and .surface(theme) → an eui-core Surface to lay out and paint
```

Changes: `ui.applied`, `ui.rejected` (issues, whether a last good version
stays), `ui.removed`, `theme.applied`, `theme.rejected`. Try it: `ede-ui
--factory apps/ede/ui check` on a fresh directory creates it with the
factory commits; `ede-ui watch` in one terminal, edit `panel.json` in an
editor (gedit, vim: both save through a rename) and see `ui.applied`, break
it and see `ui.rejected` with `"fallback": true`, then `ede-ui undo` and
`git -C ~/.config/ede/ui log`.

## Tests

```bash
cargo test --workspace   # headless: real clients, real input, the pixels pixman drew
                         # tests/lock.rs: a placeholder lock client, idle, protected windows
```

Build deps: `libxkbcommon-dev libpixman-1-dev libwayland-dev pkg-config`
(`xwayland` + `libegl1` at runtime and for the Xwayland test; Ubuntu's xwayland
package does not pull in libEGL, and Xwayland aborts without it);
the `udev` feature adds `libudev-dev libinput-dev libgbm-dev libseat-dev
libdrm-dev`.
