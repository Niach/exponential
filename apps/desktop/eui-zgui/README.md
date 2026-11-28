# eui-zgui (trial, #EDE-279)

An eUI renderer on [zgui](https://github.com/zortax/zgui) (pinned rev
`ee29c7dc`, upstream HEAD on 2026-11-28) behind `eui-core`: eui-core keeps the
reducer, theme, taffy layout and windowed List; this crate is the measurer
(zgui's parley shaper), the painter (a retained mirror of the core's placed
nodes as absolutely positioned zgui elements) and the window host.

Its OWN Cargo workspace: not a member of `apps/desktop/Cargo.toml`, not linked
by the app, the CLI or any CI workflow. Results: [RESULTS.md](RESULTS.md).

## Commands (Linux and macOS alike; winit/wgpu, Metal on macOS)

```sh
cd apps/desktop/eui-zgui
export CARGO_TARGET_DIR=$PWD/target          # keep it out of the desktop workspace's target
cargo build --release --bins

# 1. shots: the gpui reference first (it writes sizes.json), then zgui at the same sizes
mkdir -p ../crates/eui-shot/examples && cp gpui/zgui_ref.rs ../crates/eui-shot/examples/   # run-only copy, never commit it there
(cd .. && cargo build --release -p eui-shot --example zgui_ref)
../target/release/examples/zgui_ref shots /tmp/shots-gpui # or $CARGO_TARGET_DIR of the desktop workspace
target/release/zshot all /tmp/shots-zgui /tmp/shots-gpui/sizes.json
target/release/compare dirs /tmp/shots-gpui /tmp/shots-zgui /tmp/shots-cmp   # table.md, results.json, *.diff.png, *.side.png

# 2 + 4. benches (both renderers through eui-core; medians of 5 = run each 5 times)
SPIKE_T0=$(date +%s%N) target/release/zbench bench250    # cold start, one-prop delta, RSS
target/release/zbench benchlist                          # 10k fixed rows, 100 viewports
target/release/zbench benchvar                           # 10k chat rows of 1-8 wrapped lines
target/release/zbench window 15                          # idle CPU: real window, self-exits
SPIKE_T0=$(date +%s%N) ../target/release/examples/zgui_ref bench250   # same for gpui
../target/release/examples/zgui_ref benchlist
../target/release/examples/zgui_ref benchvar
../target/release/examples/zgui_ref window 15

# 3. two surfaces in one process / one window
cargo test --release --test two_surfaces
target/release/zbench two 15                             # the same, in a real window
```

On Linux every build runs `nice -n 19` inside `scripts/lanes/lane.sh heavy <owner> -- …`.
