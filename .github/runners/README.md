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

## Next: the Exponential daemon serves these (#EXP-1617)

A team device whose CI runner is on (Device settings › CI runner) ALSO serves
GitHub Actions for every team repository with **Self-hosted GitHub Actions on
team devices** on (the repo's CI settings). The daemon keeps one official
`actions/runner` (pinned, sha256-checked) per repository, `--ephemeral`, with
the labels `self-hosted` + its OS + the device's runner labels. So giving the
Mint the device label `exp-linux` and the MacBook `exp-mac` keeps
`pick-runner.yml` working unchanged. Registration tokens come from the GitHub
App, which needs **Administration: write** on the repository for that. The
Docker runners below stay: the dogfood (next section) says heavy legs need a
dedicated runner device first.

A device that serves Actions needs what the Actions jobs assume: the same
toolchain and passwordless `sudo` (`apt-get install` steps). The one real
registration on the Mint (2026-11-13, `mint-ci-exponential`, labels
`self-hosted,linux,exp-linux`) took a `desktop` job of #2798 while mint-1/2
were down and failed it on `sudo`. Switching the repo off deregistered it as
designed.

## eCode CI dogfood on the Mint (#EXP-1812, 2026-11-13)

Our own CI (`.exponential/ci.json`, the `ecode …` checks, informational: never
required) ran on the Mint's CLI daemon (`mint-ci`, 1 slot, label `exp-linux`)
beside Actions' `gate`, on every PR of Niach/exponential.

**Full gate on a shared desktop: not viable.** The four `web unit` shards took
17–30 min each (2 min unthrottled, ~4 min on Actions) at load average 66 on 32
cores, with the runner at SCHED_IDLE, and died on vitest worker timeouts. A
backlog of 9 runs built up. So `ci.json` keeps the cheap legs only (its
`$comment` says why), and a new PR head cancels its older pending runs
(#EXP-1824).

**Cheap legs, 2026-11-13 16:31 → 2026-11-14 01:16 UTC: 28 runs on 18 PRs**
(#2758, #2811–#2827). Legs: typecheck, contract-ts, merge-train,
release-scripts, rewrite-gaps; each compared with the Actions job it ports
(`web / typecheck` and the same names; Actions skips some by paths).

| Outcome | Runs | |
| --- | --- | --- |
| Every covered leg agrees with Actions | 20 | including one true red: contract-ts on #2815 at `c5baa617`, red on both |
| Superseded (a newer head on the same PR) | 4 | runs on #2815, #2816, #2818, #2826; Actions cancelled two of those heads too |
| `runner_lost` on typecheck (runner bug, see below) | 4 | #2817, #2758, #2820, #2825; Actions' `web / typecheck` green on the three that have a `gate` run |
| A leg disagreeing with Actions | 0 | |

`gate` itself was red on 5 heads where ours was green, all from legs outside
the subset: `web unit` (`bcd10ca7`, `e25f2843`, `c02949fd`), `desktop`
(`c02949fd`) and a cancelled Actions run (`e1c54ba8`).

Timing (queue → last job): ours median 53 s, p90 189 s; Actions' `test` run
median 398 s, p90 1592 s (mint-2 was offline at times while a game ran).
26 of 28 runs used CLI 0.14.158 (the mirror lock and the runner token in the
environment, #EXP-1824); the first two (#2811, #2812) predate it. The
daemon's idle memory went from ~990 MB to ~73 MB in 0.14.158 (#EXP-1821).

**What the dogfood fixed:**

- Parallel jobs raced `git init` on a fresh mirror: one lock per mirror per process.
- The Actions runner token was visible in `ps`: it rides `ACTIONS_RUNNER_INPUT_TOKEN`.
- A GitHub repo's CI is opt-in (`ci_config.enabled`), and nothing is planned without a runner device (#EXP-1820): rows nobody could take showed as pending checks.
- `runner_lost` under load: the whole daemon ran SCHED_IDLE, so at load 60+ its beats starved and the server failed live jobs after 2 min. Now the runner runs each step at SCHED_IDLE + nice 19 and keeps its own policy.
  - The unit must stop idling the daemon: no `CPUSchedulingPolicy=idle`, and `CPUWeight=1` instead of `idle`. A cgroup at `idle` starves every thread in it.
- The desktop repo page's Started column was empty: rows from the sync store spell timestamps the Postgres way (#EXP-1826).

**Recommendation:** run heavy legs (web unit, desktop, android) on a dedicated
runner device, not on someone's desktop. A desktop runner is fine for the
cheap legs, at idle priority for the steps only.

**Next step:** our CI is OFF for Niach/exponential again (`ci_config.enabled:
false`; mint-ci's runner stays on). Re-enable it after the CLI release that
carries the idle steps AND the unit change above
(`~/.config/systemd/user/exponential-ci-daemon.service`: drop
`CPUSchedulingPolicy=idle`, `CPUWeight=1`; then `systemctl --user
daemon-reload && systemctl --user restart exponential-ci-daemon`).

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

| What | Where | Priority | Memory |
| --- | --- | --- | --- |
| CI runners | `expci.slice` (root level) | `CPUWeight=idle`, `IOWeight=10`, 12 + 12 cores max | `MemoryHigh=16G`, `MemoryMax=20G`; 12 GB per container, no swap; `oom_score_adj` 1000 |
| Agent runs + their builds, lane builds, emulators | `user@1000.service/expbg.slice/run-*.scope` (`run-scope-background.conf` moves every `systemd-run --user --scope` there) | `CPUWeight=idle` + nice 10; lanes nice 19, `ionice -c 3` | the slice: `MemoryHigh=16G`, `MemoryMax=20G` |
| mint-ci daemon (eCode CI) + its jobs | `user@1000.service/expbg.slice/exponential-ci-daemon.service` | daemon nice 19 at `CPUWeight=1` (never `idle`: its beats starve into `runner_lost`); its job steps SCHED_IDLE + nice 19 (the runner sets them) | `MemoryMax=10G` inside the slice; `OOMScoreAdjust=1000` |
| Cinnamon session (desktop app, browser, the Steam client) | `session-<id>.scope` | `CPUWeight=1000` (`session-interactive.conf`) | uncapped |
| Steam games (Dota 2 = `app-steam-app570-*`) | `user@1000.service/app.slice/app-steam-app*.scope` (Steam makes them; not movable) | `CPUWeight=1000` against app.slice (`app-steam-game.conf`); idle background never wins a core | uncapped |
| Everything else in user@ (flatpaks, pipewire) | `user@1000.service` | default 100; pipewire's RT threads ignore weights | uncapped |

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

#### Memory and games (2026-11-13)

The Mint hard-reset with no log twice (2026-11-10, 2026-11-13 12:45) while
Dota 2 ran next to CI. It has 62 GB of RAM and 2 GB of swap, no zram, and had
no memory cap anywhere. Measured: both runners peaked at 27 GB together
(18 + 16), one agent run with gradle daemons and an Android emulator held 23 GB
(14 GB anon), Dota 6–15 GB, the Cinnamon session 5.5–8 GB. Together that is
more than the machine has. Now the background can never take more than 40 GB
(expci.slice 20 + expbg.slice 20), so the desktop and a game always keep
≥ 22 GB. At `MemoryHigh` a build gets slower (the kernel reclaims and
throttles it); at `MemoryMax` the kernel OOM-kills inside that slice only, so
a gradle or link step fails rather than the machine. systemd-oomd kills a
background slice's job after 20 s at ≥ 80% memory pressure, and never watches
`user@` as a whole (`oomd-user-service-defaults.conf` replaces Ubuntu's
default, which could pick Dota). IO weights do nothing here: the NVMe drives use
the `none` scheduler and user@ does not delegate `io`, so `IOWeight` is
documentation only.

While a game runs, CI drops to ONE Docker runner instead of stopping. Every
`app-steam-*.scope` pulls in `exp-gaming.service`. Its `gaming-ci.sh hold`
stops one runner, runner-2 first, as soon as that runner is between jobs, never
mid-job. When the last game scope ends, systemd stops the unit and
`gaming-ci.sh release` starts the runner again. A stopped runner is offline, so
`pick-runner.yml` overflows to GitHub-hosted sooner. The mint-ci daemon is held
to one job by its device runner settings. Log:
`~/.local/state/exponential/gaming-ci.log`.

```bash
bash .github/runners/linux/install-desktop-first.sh   # user half, no root; rerun to upgrade; --uninstall
systemctl --user restart exponential-ci-daemon        # between jobs: moves it into expbg.slice
```

The root half, once (fish-compatible). The two files are copied to `~/exp-runners/` first:

```bash
sudo sh -c 'install -m644 /home/niach/exp-runners/expci.slice /etc/systemd/system/expci.slice && mkdir -p /etc/systemd/system/user@.service.d && install -m644 /home/niach/exp-runners/oomd-user-service-defaults.conf /etc/systemd/system/user@.service.d/10-oomd-user-service-defaults.conf && apt-get install -y systemd-oomd && systemctl daemon-reload && systemctl enable --now systemd-oomd.service' && systemctl --user daemon-reload
# verify: 17179869184 / 21474836480, then expci.slice and expbg.slice under "Memory Pressure Monitored CGroups"
cat /sys/fs/cgroup/expci.slice/memory.high /sys/fs/cgroup/expci.slice/memory.max; oomctl | grep -E 'Path: .*(expci|expbg)'
```

Recreating a runner (compose changes) also drops its job. Check
`docker top <c> -eo pid,comm | grep Runner.Worker` first (`-eo comm` alone
errors) or use `recreate-when-idle.sh`.

A hard reset with nothing in the journal can also be the hardware: run
memtest86+ overnight from the GRUB menu (Esc at boot on UEFI, Shift on BIOS; `sudo apt install memtest86+` if it is missing), check
`sudo dmesg | grep -iE 'edac|mce|hardware error'` for corrected memory errors,
and if XMP/EXPO is on, try one boot at JEDEC speed or one step lower: 64 GB at the XMP/EXPO rating is often only
marginally stable.

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
