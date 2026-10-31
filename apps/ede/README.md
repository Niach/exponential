# eDE

The Exponential Desktop Environment (#NEW-10): a Wayland compositor plus a
shell drawn in eUI. This is its own Cargo workspace; it depends on the shared
libraries, never the other way round.

| Crate | Role |
|---|---|
| `ede-compositor` | smithay compositor: floating windows with magnetic edges and edge tiling, xdg-shell, wlr-layer-shell, damage-tracked rendering, `~/.config/ede/compositor.toml`. Backends: `winit` (nested, development) and `headless` (CI). |

## Run it nested (Cinnamon/X11)

```bash
cd apps/ede
cargo run -- -- gnome-calculator     # anything after `--` starts inside eDE
WAYLAND_DISPLAY=wayland-1 xed        # or point a client at the socket it logs
```

The compositor opens one window on the running X11 (or Wayland) session and
serves `$XDG_RUNTIME_DIR/wayland-N`. Close the window to quit.
`--headless 1920x1080` runs without a window (pixman, no GPU).

Bindings: Alt+drag moves a window, dropping it on a side edge tiles it to a
half (corners = quarter, top = maximize), Super+Left/Right tile, Super+Up
maximizes, Super+Down restores, Alt+F4 closes.

## Config

`$XDG_CONFIG_HOME/ede/compositor.toml`, every key optional; a broken file is
logged and ignored.

```toml
background = "#0a0a0a"

[keyboard]
layout = "de"
repeat_delay = 400
repeat_rate = 30

[windows]
snap_threshold = 12   # px; 0 = off
edge_tiling = true
```

## Tests

```bash
cargo test --workspace   # headless: real clients, real input, the pixels pixman drew
```

Build deps: `libxkbcommon-dev libpixman-1-dev libwayland-dev pkg-config`.

Next: the udev/DRM + libinput backend for a real login session and
multi-monitor hotplug, then Xwayland (#NEW-102) and `ede-session` (#NEW-103).
