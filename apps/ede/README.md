# eDE

The Exponential Desktop Environment (#NEW-10): a Wayland compositor plus a
shell drawn in eUI. This is its own Cargo workspace; it depends on the shared
libraries, never the other way round.

| Crate | Role |
|---|---|
| `ede-session` | What the login screen starts: the session environment (`XDG_CURRENT_DESKTOP=eDE`, Wayland hints for Firefox/Qt/Electron), `ede-compositor` on the seat, then `ede-session.target` (XDG autostart via systemd's generator, polkit agent, gnome-keyring). Restarts a crashed compositor; three quick crashes in a row (also across logins) bring it back in safe mode. |
| `ede-compositor` | smithay compositor: floating windows with magnetic edges and edge tiling, fullscreen, xdg-shell, wlr-layer-shell, Xwayland (X11 apps and games), linux-dmabuf, drm-syncobj explicit sync (udev), ext-session-lock, idle notify/inhibit, input methods (input-method-v2, text-input-v3, virtual keyboard), protected surfaces, damage-tracked rendering, `~/.config/ede/compositor.toml`. Backends: `winit` (nested, development), `udev` (the real session: DRM/KMS, libinput, hotplug; feature `udev`) and `headless` (CI). |

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

Checks the GPU render nodes, Xwayland (24.1+ has explicit sync; older
versions flicker with X11 games on NVIDIA), and on NVIDIA that
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

Log out; LightDM's session menu now has **Exponential** and **Exponential
(safe mode)** next to Cinnamon. Safe mode starts with the stock settings
(`--safe-mode`: the config file is not read; `EDE_SAFE_MODE=1` for the
shell). Super+Shift+E logs out; Ctrl+Alt+F1…F12 switch VTs. Logs:
`journalctl --user -b` and `~/.xsession-errors`. Remove with `sudo apt
remove ede`; Cinnamon is untouched.

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

The lock screen is a client of `ext-session-lock` (the eUI shell's, later;
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
