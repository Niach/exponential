# Exponential UI: the gpui sample

This is a plain gpui window that hosts an Exponential UI surface. The
surface streams from the local A2UI JSONL server (`../server`) and uses the
server's custom theme plus ONE custom extension component (`TrendLine`),
painted natively. It needs no Exponential account and no backend of ours.

It is a standalone Cargo project with its own `[workspace]`, so it is not
part of `apps/desktop`'s build graph. It uses the SDK the way a third party
does:

- `exponential-ui` (the core) and `exponential-ui-gpui` (the painter + host
  runtime) as path dependencies. These stand in for crates.io until gpui and
  gpui-component are published there.
- gpui + gpui-component as git dependencies at the SDK's revisions.
  `Cargo.lock` started as a copy of `apps/desktop/Cargo.lock`, so every git
  dependency resolves to the same commit.
- `rust-toolchain.toml` pins Rust 1.96.0, the compiler gpui needs.

## Run

```sh
bun samples/exponential-ui/server/server.ts &   # http://localhost:4190
cd samples/exponential-ui/gpui
cargo run              # the surface once (?once=1), deterministic
cargo run -- --live    # the live stream: a data tick every 3 s
cargo run -- --dark    # dark mode
EXP_SAMPLE_SERVER=http://host:4190 cargo run
```

## What `src/main.rs` does

It mirrors `../web/src/main.jsx`:

1. Fetches `/theme.json`, loads it with `exponential_ui::theme::load_theme`
   over the built-ins (`extends: neutral`), and fetches `/extension.json`
   (`parse_extension`).
2. Builds an `ExponentialHost`:
   - a `JsonlStreamTransport` on `/a2ui.jsonl` with `post_url` = `/action`
     (client messages, e.g. the `refresh` action, are POSTed back);
   - the extension catalog and its native painter;
   - the theme in light mode.

   Then it calls `connect`.
3. Opens ONE gpui window that paints `host.surface("greenhouse")` (a
   `SurfaceView`) and the transport status.

The `TrendLine` painter is an `ExtensionPainter`. It fills the offered
width, is `height` tall (default 48), and strokes a gpui `canvas` path
through the bound `/series` values in `color`, else the theme's primary.
