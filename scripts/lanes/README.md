# Lanes

One machine, many agent sessions: the lanes keep them from fighting over the
simulator, the emulator and the CPU. `lock.sh` is the one implementation;
`lane.sh` runs one command inside a lane and always releases what it took.

```sh
scripts/lanes/lane.sh <ios|android|heavy> <owner> [--own-target] -- <command...>
```

- **ios / android:** the simulator and the emulator, one holder each.
- **heavy:** a heavy build, test or perf run. One holder by default; a machine
  with the cores and disk for more says so in its config
  `~/.config/exponential/lane-slots` (`heavy=2`; survives a reboot, unlike
  `/tmp`), and `lane.sh` takes any free slot. A
  `${EXP_LANE_LOCK_ROOT:-/tmp}/exp-lane-slots` file overrides it for one boot.
  Mint runs `heavy=1` (a second slot was ruled out, #EXP-1844); the MacBook
  has no file (1).
- **Locks** record owner, start time, pid, host and session. A waiter on the
  same host takes over a lock whose pid is gone. Release only your own
  (`release_lock <dir> <owner>`); never `rm` a lock.
- **Max hold (heavy only, #EXP-1844):** `LANE_MAX_HOLD_SECONDS` (default 2700
  = 45 min, `0` = none). At the deadline the holder's `lane.sh` warns on its
  stderr and logs `overrun`; the next waiter then ENDS the holder's lane.sh
  and its whole process tree (TERM, KILL after `LANE_KILL_GRACE_SECONDS`,
  default 20; logged `end`) and takes the slot over (`handover`). A holder it
  cannot end (not a lane.sh of this host, or a survivor) keeps the slot
  (`handover-refused`): a lane never runs two heavy jobs (#FEED-306: on
  2026-11-24 an overrun cargo build kept running after its handover and held
  mint's `expbg.slice` over its memory.high for four hours). The simulator and
  emulator lanes have no limit.
- **A lane job goes with its lane.sh and its session (#FEED-306):** lane.sh
  runs the command in the background and ends its whole tree on any exit (a
  signal, Ctrl-C, the command's own end). When its caller is gone (a stopped
  turn's shell: lane.sh reparented to pid 1) it ends the job and itself
  within `LANE_ORPHAN_POLL_SECONDS` (5; logged `orphaned`), and stops waiting
  for a slot. It lists itself in the session manifest (`lane <pid>`, removed
  at its end), so the daemon ends the tree when the session ends, on macOS
  too.
- **First come, first served (#EXP-2055):** a free or handed-over slot goes
  only to the oldest live waiter in the queue (`exp-lane-waiters`, one file per
  waiting pid; all of a lane's slots share slot 1's queue), whatever its poll
  interval. A dead waiter's entry is pruned; one that stopped polling (no touch
  for 2 polls + 60 s) is skipped. An owner that already holds a slot of the
  lane queues behind everyone else.
- **Who holds what:** `scripts/lanes/lane.sh status` prints each slot's holder
  (owner, session, pid, held for) and the waiters; a waiting run names the
  holder the same way. Every take, wait, release, handover and overrun is
  appended to `~/.local/state/exponential/lanes.log` (`LANE_LOG` overrides it),
  with wait and hold times.

- **Disk:** once it holds a slot, `lane.sh` runs `exponential sweep-target
  --if-low "$CARGO_TARGET_DIR"` before the command: while the target's disk is
  under `cargoSweep.freeDiskGb` (settings.json, default 20) it deletes stale
  build output from the shared target (#EXP-1865, the daemon's rule), else
  nothing.
  It takes the first CLI that knows `sweep-target`: `$LANE_SWEEP_CLI`, each
  `exponential` on `PATH`, then the target's own `release/` or `debug/`
  build (`sweep.sh`, #EXP-1996). With none, the lane says
  `lane.sh: NOT sweeping …` and names the binary that is too old: install a
  current CLI (`curl -fsSL https://exponential.at/install.sh | sh`).

## Screen captures: `capture.sh` only (#EXP-2022)

A Linux screen capture goes through `capture.sh`, never a bare `import`,
`xwd`, `scrot`, `grim` or `gnome-screenshot`: on 2026-11-19 an
`import -window root` with the inherited `DISPLAY=:0` photographed the
person's live desktop.

```sh
Xvfb :77 -screen 0 1280x800x24 -nolisten tcp &   # your own display
scripts/lanes/capture.sh :77 /tmp/s/shot.png [1280x800]
scripts/lanes/capture.sh --window "$W" :77 /tmp/s/win.png
```

It refuses (exit 2, writes nothing) an empty display, `:0`, `wayland-*`, a
`host:N`, the caller's inherited `$DISPLAY`/`$WAYLAND_DISPLAY`, and any display
not served by an Xvfb (its `/tmp/.X<N>-lock` pid). It captures with the
inherited variables unset and compares the picture with the expected geometry
(the argument, else the Xvfb's or the window's own); a mismatch deletes it
unviewed (exit 3). Inside `xvfb-run`, pass `"$DISPLAY"` with the variables
unset: `D=$DISPLAY; env -u DISPLAY -u WAYLAND_DISPLAY capture.sh "$D" out.png`.
A headless Wayland compositor uses its own capture (`exponential-ctl
capture`); grim only with an explicit socket that is neither `wayland-0` nor
the inherited one (the check is in the script's header). iOS `simctl` and
Android `adb` captures can't see the desktop and stay as they are.

## Own target for anything that must be YOUR code

Dev machines export one shared `CARGO_TARGET_DIR` so worktrees share a build
cache. That is fine for `cargo check` and plain tests, but NOT for pixel
benches, captures or perf runs:

- Cargo leaves a path crate's directory out of its artifact hash, so two
  worktrees' `ui` crates compile to the same artifact name. A build from your
  worktree can find the other worktree's artifact and count it as fresh,
  because its recorded sources (the other checkout's files) haven't changed.
- The final binaries (`target/<profile>/<bin>`) are overwritten by whichever
  worktree linked last.

So a bench binary may not be your code. A Board detail bench once reported an
impossible 0 px this way. Run those commands with `--own-target`: it sets
`CARGO_TARGET_DIR` to this checkout's `apps/desktop/target` (gitignored) for
the command alone.

```sh
scripts/lanes/lane.sh heavy my-session --own-target -- cargo test -p ui settings_bench
```

The first own-target build is a full one. Delete the directory when the work is
done (`rm -rf apps/desktop/target`) to give the disk back. The desktop capture
(`packages/shots`) already refuses a binary that another checkout linked
(EXP-1558).

## Session manifest: what a session leaves outside its cgroup

When a session ends, the daemon stops its cgroup scope, which takes every
process it started (#EXP-1868). Things a session creates **outside** that scope
survive it unless they are listed: iOS simulators (~30 GB each), Android AVDs,
DerivedData and `/tmp` build dirs, compose projects and their volumes. Whoever
creates one lists it in the session's manifest:

```sh
scripts/lanes/session-manifest.sh add sim "$(xcrun simctl create exp-s339 …)"
scripts/lanes/session-manifest.sh add avd exp_s339_pixel
scripts/lanes/session-manifest.sh add dir /tmp/s339-dd
scripts/lanes/session-manifest.sh add compose cap9122
```

The manifest lives at `~/.local/state/exponential/session-manifests/<session>`.
When the session ends, the daemon deletes exactly what it lists and nothing
else. That covers `simctl delete`, `avdmanager delete avd`, `rm -rf`, and
`docker compose -p … down --volumes`. A `dir` must sit under a temp dir,
Xcode's DerivedData, fastlane's caches or the session's worktree. If the daemon
crashed, its sweep reaps the manifest the same way. This works the same on
macOS and Linux.

These already list what they create:
- the Fastfile: a non-`store` lane tag's simulators and caches, and a
  `SNAPSHOT_DERIVED_DATA` override
- `apps/runner/ios` checks: their simulators
- the desktop capture: its temp dirs

Outside a session (your own shell, CI) `add` records nothing.
