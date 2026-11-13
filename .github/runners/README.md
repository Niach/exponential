# Self-hosted runners

The heavy jobs run on our own machines when they are on, and on GitHub-hosted
runners when they are not: `workflows/pick-runner.yml` asks GitHub which
runners are online and hands every workflow a `runs-on`. A pull request from a
fork never runs any workflow (the same job skips, everything needs it).

| Label | Machine | Takes |
|---|---|---|
| `exp-linux` | the Mint, 2 Docker runners (`linux/`) | test: desktop, android · build-desktop Linux · build-android · exponential-ui rust |
| `exp-mac` | the MacBook, native (`mac/`) | test-ios · exponential-ui swift, swift-ios · release-eui swift (the xcframework, unsigned) |

Stays on GitHub: build-web (pushing an image from the Mint goes through a
home upload link), the exponential-ui compose leg (its snapshots are pixel-exact
to the ubuntu image they are recorded on), the web unit shards (4 free parallel runners beat one
machine), every signed Apple artifact (build-ios, the desktop .app:
App Store review rejects a beta-macOS host and the signing steps rewrite the
user's keychain list), build-cli (its glibc floor is Ubuntu 22.04), arm64 and
Windows.

## Secret

`RUNNER_STATUS_TOKEN` (repo secret): a fine-grained token on Niach/exponential
with **Administration: read**, so `pick` can list the runners. Missing or
expired = every job falls back to GitHub-hosted.

## The Mint

```bash
cd .github/runners/linux
RUNNER_TOKEN=<token> docker compose up -d --build   # a runner's first start
docker compose up -d --build                        # later: rebuild/upgrade
```

`<token>` = `gh api -X POST repos/Niach/exponential/actions/runners/registration-token -q .token`
(valid one hour). Two runners: the Mint is Danny's desktop, and four at 16
CPUs each made it lag (2026-11-03). Since 2026-11-07 the desktop wins by
priority: the runners run in `expci.slice` (`CPUWeight=idle`, a sibling of
user.slice), so CI only gets the cores nobody else wants. Install the slice
before the first `up` (`sudo cp expci.slice /etc/systemd/system/ && sudo
systemctl daemon-reload`). `RUNNER_CPUS` in a `.env` next to `compose.yaml`
caps EACH runner (default 6; the Mint: 12).

### Desktop first (PERF-4, 2026-11-12)

On the Mint, an IDE frame that needed 180 ms of CPU took 2.1 s at load average
25–50. The Mint has 32 cores. Most of that load is background work, ranked
so the desktop always wins:

| What | Where | Priority |
| --- | --- | --- |
| CI runners | `expci.slice` (root level) | `CPUWeight=idle`, `IOWeight=10`, 12 + 12 cores max |
| Agent runs + their builds | `user@1000.service/app.slice/run-*.scope` | `CPUWeight=idle` + nice 10 (the desktop and CLI set both from the release after PERF-4, `crates/eagent` `run_scope`; `run-scope-background.conf` until then) |
| Lane builds (`scripts/lanes/lane.sh`) | inside the agent's scope | nice 19, `ionice -c 3` |
| Cinnamon session (desktop app, browser) | `session-<id>.scope` | `CPUWeight=1000` (`session-interactive.conf`) |
| Everything else in user@ (games, flatpaks, pipewire) | `user@1000.service` | default 100; pipewire's RT threads ignore weights |

The measurements (`/proc/<pid>/task/<pid>/schedstat`, a probe burning 180 ms of
CPU every 50 ms, 48 busy threads as the "build"):

- 48 threads in an agent-like user scope at weight 100: the 180 ms frame
  took 416 ms median, 692 ms p90, 1.4 s max. The desktop's main thread waited
  49% of its run time, Cinnamon 20% (idle machine: 3% and 1%).
- The same scope at `CPUWeight=idle`: 181 ms median. That protects only its
  siblings in app.slice, though. The session scope sits one level up, where
  user@1000.service still competes at full weight, so the desktop still
  waited 39%. Hence `CPUWeight=1000` on the session: a weight-1000 scope next
  to the default-weight burners kept 181 ms median, 217 ms max.
- The runners' slice name matters. `exp-ci.slice` (the 2026-11-07 name) is
  `exp.slice/exp-ci.slice` to systemd, so idle ranked it only against its own
  siblings. The Mint's containers ran there until 2026-11-12 (recreated into
  `expci.slice` by `recreate-when-idle.sh`, between jobs). Check with
  `docker inspect -f '{{.HostConfig.CgroupParent}}' exp-runners-runner-1-1`.

No CPU quota beyond `RUNNER_CPUS`: an idle-weight group gives a core back the
moment anything else wants it. A quota would only slow CI while the desktop
sits idle.

Overflow: when the exp-linux jobs already waiting exceed two per online
runner, `pick-runner.yml` sends the run to GitHub-hosted instead (input
`linux-overflow`; 2026-11-03..07 the Mint queued 16 min at p90).

Disk: `prune.sh` (a weekly user crontab line, see its header) drops images
nothing used for a week and build cache above 20 GB, never volumes. The caches (sccache, cargo, rustup,
gradle, bun, tool cache) are named volumes (per runner except sccache, the tool
cache and the read-only Gradle dependency cache); `docker compose down -v`
drops them and the registration. sccache is capped at 15 GB
(`SCCACHE_CACHE_SIZE`, compose + Dockerfile).

Between jobs, a systemd user timer keeps the whole Mint above 60 GB free
across the runners and every session (2026-11-12: 241 GB free on 11-08, 9.5 GB
four days later: 107 GB of build cache the weekly prune never matched, ~80 GB
of per-session cargo targets outside their worktrees, the shared target's test
binaries, a 43 GB sccache):

```bash
bash .github/runners/linux/install-disk-keeper.sh              # rerun to upgrade; --uninstall
bash .github/runners/linux/disk-keeper.sh --dry-run             # what it would delete now
tail ~/.local/state/exponential/disk-keeper.log                 # every removal + WARN lines
```

Every 15 minutes, below 80 GB free, it deletes only regenerable build output
until 80 GB is free: Docker build cache above 20 GB and dangling images (never
containers or volumes: other projects' data lives there), scratch cargo
targets outside any worktree (`~/.cache/<x>-target`, `/tmp/<x>/target-*`, the
ones agents point `CARGO_TARGET_DIR` at; nothing removes them when the session
ends) whole once nothing wrote to them for two days and no process has them
open, then cargo `deps` files and `incremental/` dirs older than a day (those
scratch targets, idle worktrees' `target*`, the shared CARGO_TARGET_DIR; a
profile only while no build holds its `.cargo-lock`), the runners' work-dir
targets the same way while the runner runs no job, Gradle build-cache entries
older than a day and idle worktrees' Android `build/`, the user's top-level
`/tmp` entries nothing wrote to for a day and no process has open, idle
worktrees' `node_modules`, lane AVDs (`exp*`) untouched for two days with no
emulator on them, and cargo git checkouts unused for three days. Worktrees are
live by the Mac's rule (below); the session's own worktree goes on session end
(`crates/coding` `run_cleanup`). Still below 60 GB after cleaning = a `WARN`
line and one desktop notification an hour. The journal (2.4 GB) needs root:
`SystemMaxUse=1G` in `/etc/systemd/journald.conf` caps it.

## The MacBook

```bash
bash .github/runners/mac/install.sh
```

A LaunchAgent under your user, niced (never Background QoS: it pins jobs to the efficiency cores). Builds with the newest release Xcode 27.x
(`/Applications/Xcode.app`, through `DEVELOPER_DIR`) on the iOS 27 simulator
only (`.github/scripts/select-xcode.sh`; no beta Xcode, no other runtime) and Homebrew tuist (test-ios fails loudly if tuist is off the pin).
`~/actions-runner/svc.sh stop` pauses it; jobs then fall back to GitHub.

Two instances (2026-11-03), so a release build never queues the PR jobs:
`macbook` in `~/actions-runner` and `macbook-2` in `~/actions-runner-2`
(`bash .github/runners/mac/install.sh macbook-2`), same labels, each its own
launchd service (`~/actions-runner-2/svc.sh stop` pauses only it). What they
must not share, the script splits per instance: the work dir (so checkouts,
`apps/desktop/target` and every DerivedData differ: test-ios builds into
`$RUNNER_TEMP/dd` and `$RUNNER_TEMP/dd-test`, never `~/Library/Developer/Xcode`,
whose kept GRDB modulemap is read-only and fails Copy Module Map), the tuist
and SwiftPM caches (`TUIST_XDG_CACHE_HOME`, `EXP_SWIFTPM_CACHE` =
`~/actions-runner-2/_cache{,/swiftpm}`; test-ios's Actions cache of `Tuist/.build` is keyed on `github.workspace`, its `workspace-state.json` holds absolute paths), the sccache server port (4226, 4227; the cache dir stays shared),
`TMPDIR` (`~/actions-runner-2/_tmp`: the swift-ios bench's fixed DerivedData)
and the simulator (`IOS_SIM_DEVICE_NAME` = `iPhone 17 Pro Max (macbook-2)`;
swift-ios shuts down the device it booted). Shared on purpose: rustup, the
cargo registry, Homebrew tuist, Xcode. Bench timings on a Mac running two jobs
are noisier (they only warn).

Disk (EXP-1396): every Mac job runs `.github/scripts/disk-guard.sh` first.
Below 25 GB free it cleans (DerivedData older than a day, cargo `incremental/`
dirs, unavailable simulators, dangling docker images, then
`~/Library/Caches/sccache-ci`); below 10 GB it fails the job. By hand:
`--check` (exit 1 below 25 GB), `--dry-run` (lists, deletes nothing).

Between jobs, a LaunchAgent keeps the whole Mac above 30 GB free across every
thread (2026-11-06: 5 GB free stopped OrbStack and every stack):

```bash
bash .github/runners/mac/install-disk-keeper.sh               # rerun to upgrade; --uninstall
bash .github/runners/mac/disk-keeper.sh --dry-run              # what it would delete now
tail ~/Library/Logs/exponential/disk-keeper.log                # every removal + WARN lines
```

Every 15 minutes, below 40 GB free, it deletes only stale build output until
40 GB is free: cargo `deps` files (every profile and triple) and `incremental/`
dirs older than a day (idle worktrees' `target*` dirs, then the shared
`CARGO_TARGET_DIR`, a profile only while no build holds its `.cargo-lock`),
DerivedData nothing wrote to for a day, idle worktrees' `apps/ios/build` and
Android `build/` dirs, Gradle build-cache entries older than a day, unavailable
simulators, finished capture stacks (containers exited over a day ago, then
unused volumes; never the `exponential` stack), the user's `/tmp` scratch nothing
wrote to for a day and no process has open (lanes' `-derivedDataPath /tmp/…`,
capture stacks: ~9 GB a day), idle worktrees' `node_modules` and
`apps/ios/Tuist/.build` (~5 GB of SwiftPM artifacts each), `exp-*` lane
simulators untouched for two days (never `exp-store`) and cargo git checkouts
unused for three days. The runners' sccache is capped at 15 GB (`install.sh`). A worktree is live, never touched, when it is locked,
holds an `index.lock`, is a process's cwd, had a file or git write in the last
day, or has anything uncommitted; in an idle one only git-ignored paths go.
Still below 30 GB after cleaning = a `WARN` line in the log and one
Notification Center banner an hour. The MacBook only, never the server.

## Off switch

Delete the `RUNNER_STATUS_TOKEN` secret: everything runs GitHub-hosted again.
