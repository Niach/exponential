# Self-hosted runners

The heavy jobs run on our own machines when they are on, and on GitHub-hosted
runners when they are not: `workflows/pick-runner.yml` asks GitHub which
runners are online and hands every workflow a `runs-on`. A pull request from a
fork never runs any workflow (the same job skips, everything needs it).

| Label | Machine | Takes |
|---|---|---|
| `exp-linux` | the Mint, 1 Docker runner (`linux/`) | test: desktop, android · build-desktop Linux · build-android · exponential-ui rust |
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
(valid one hour). One runner on purpose: the Mint is Danny's desktop, and four
made it lag (2026-11-03). `RUNNER_CPUS` in a `.env` next to `compose.yaml` caps
it (default 6; the Mint: 16 of its 32 cores). The caches (sccache, cargo,
rustup, gradle, bun, tool cache) are named volumes; `docker compose down -v`
drops them and the
registration.

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
`~/actions-runner-2/_cache{,/swiftpm}`), the sccache server port (4226, 4227; the cache dir stays shared),
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

## Off switch

Delete the `RUNNER_STATUS_TOKEN` secret: everything runs GitHub-hosted again.
