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
  Mint runs `heavy=2`; the MacBook has no file (1).
- **Locks** record owner, start time, pid and host. A waiter on the same host
  takes over a lock whose pid is gone. Release only your own
  (`release_lock <dir> <owner>`); never `rm` a lock.

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
