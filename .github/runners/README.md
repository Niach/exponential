# Self-hosted runners

The heavy jobs run on our own machines when they are on, and on GitHub-hosted
runners when they are not: `workflows/pick-runner.yml` asks GitHub which
runners are online and hands every workflow a `runs-on`. A pull request from a
fork never runs any workflow (the same job skips, everything needs it).

| Label | Machine | Takes |
|---|---|---|
| `exp-linux` | the Mint, 2 Docker runners (`linux/`) | test: desktop, android · build-web amd64 · build-desktop Linux · build-android · exponential-ui rust, compose |
| `exp-mac` | the MacBook, native (`mac/`) | test-ios · exponential-ui swift, swift-ios |

Stays on GitHub: the web unit shards (4 free parallel runners beat one
machine), every signed Apple artifact (build-ios, the desktop .app, release-ui:
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
RUNNER_TOKEN=<token> docker compose up -d --build   # first start only
docker compose up -d --build                        # later: rebuild/upgrade
```

`<token>` = `gh api -X POST repos/Niach/exponential/actions/runners/registration-token -q .token`
(valid one hour). `RUNNER_CPUS` in a `.env` next to `compose.yaml` caps the
jobs (default 6). The caches (sccache, cargo, rustup, gradle, bun, tool
cache) are named volumes; `docker compose down -v` drops them and the
registration.

## The MacBook

```bash
bash .github/runners/mac/install.sh
```

A LaunchAgent under your user, background priority. Uses your selected Xcode
and Homebrew tuist (test-ios fails loudly if tuist is off the pin).
`~/actions-runner/svc.sh stop` pauses it; jobs then fall back to GitHub.

## Off switch

Delete the `RUNNER_STATUS_TOKEN` secret: everything runs GitHub-hosted again.
