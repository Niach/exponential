# GitHub Actions

Two workflows publish ghcr images on every push to `master` and on
`v*.*.*` / `v*.*.*-dev` tags — the distribution channel for both the cloud
and self-hosting:

- `build-web.yml` → `ghcr.io/niach/exponential-web` (root `Dockerfile`,
  multi-arch amd64 + arm64 via native runners + a digest-merge job)
- `build-steer-relay.yml` → `ghcr.io/niach/exponential-steer-relay`
  (`Dockerfile.steer-relay`, multi-arch via QEMU)

Tag scheme (both images): `latest` + `master` + `sha-<short>` on master
pushes, semver (`0.18.21` / `0.18`) on tag pushes. `latest` moves on every
master push — self-hosters pin semver (`selfhost/docker-compose.yaml` +
`INSTALL.md`). Both ghcr packages must stay **public** so anonymous
`docker pull` works.

The cloud at https://app.exponential.at runs a Coolify dockerimage app that
pulls the web image; the steer relays (steer.exponential.at /
steer-next.exponential.at) pull the relay image the same way. Coolify's
control plane is home-LAN-only, so **there is no auto-redeploy webhook**.
After a green Actions run, deploy manually from a LAN-connected machine,
after the pre-deploy check against the TARGET database (FEED-149: it exits 1
on any machine more than one account shares into one team; a team owner
drops the extra share, the Team devices row's Remove, before the deploy):

```
cd apps/web && DATABASE_URL=<target database> bun run check:device-shares
coolify deploy uuid <app-uuid>
```

Or click "Deploy" in the Coolify UI.

Health (EXP-1474): `build-web.yml` boots each image leg before pushing it
(`apps/web/scripts/smoke-server.ts`: `/`, `/auth/login` must be our app
shell, `/api/health` must say `{"ok":true}`), and the image's HEALTHCHECK
matches that body. A dead server can answer `/api/health` with 200 and an
error page, so Coolify's own health check on the web app must match the
response text too: path `/api/health`, return code 200, response text
`"ok":true`. Locally after `bun run build`: `cd apps/web && bun run
smoke:server -- --boot` (needs `DATABASE_URL` + `BETTER_AUTH_SECRET`).

The device-shares gate (FEED-149/FEED-155) is a production deploy step, not
a CI job: no workflow can reach the production database. It is read-only and
lists every machine id two accounts share into one team, host row first, with
each row's id. Settle each pair before deploying: the team's owner removes
the extra share (Team devices, Remove, which is `devices.dropShare`); when the
owner does not respond, an instance admin calls `admin.dropDeviceShare({id,
teamId})` with the row id from the check's output. Rerun the check until it
exits 0. A machine whose key another account pinned first (FEED-154: its
owner's share is refused with "Another account holds this machine's key") is
reset with `admin.clearDeviceKey({deviceId})`; the box's next register pins
again. The operations runbook (credentials, Coolify app uuids, the target
database URLs) lives outside this repository.

One-off right AFTER the deploy that ships migrations 0206 + 0213 (FEED-165):
`cd apps/web && DATABASE_URL=<target database> bun run audit:device-first-seen`
(it prints the host/database it reads and refuses the local dev database
without `--dev`). It is read-only (a READ ONLY transaction) and lists every
machine id whose first-seen account (the FEED-159 pin owner) is not the
account with the earliest evidence (first session on the id, eldest
`devices` row, the pin), and every id whose pin another account holds, with
each candidate's evidence and the `admin.clearDeviceKey({deviceId, pinTo})`
call that settles it. 0213 recomputes first-seen from that same rule but
leaves three kinds of row alone, which the audit therefore lists as
mismatches ON PURPOSE: a first-seen account that holds the pin (a migration
never moves a live pin: a pre-deploy squat's pin and an owner's after a
Remove + relaunch look the same in the data), one with a `device_removals`
row for the id (its Remove deleted its original row, so an older squat row
could read as earliest), and a row an admin `pinTo` (or a register after
the deploy) wrote. Review each by hand: keep it (no call) or settle it with
the printed `pinTo`. Any OTHER mismatch means the rule and the migration
drifted. Review the pin list by hand too. Settling WITHDRAWS EVERY share of
the id, the rightful owner's included (their hosted runs end, they are
told): the owner re-shares the machine afterwards. The call is a raw tRPC
POST as an instance admin, no transformer:
`POST /api/trpc/admin.clearDeviceKey` with the JSON body
`{"deviceId":"…","pinTo":"<userId>"}`. The audit refuses localhost:54321
(the local dev DB), so a tunnel to the target database must listen on
another local port. During the
rolling deploy, old pods still register without writing first-seen (a short
window: the next register on a new pod writes it).

Steer relay order (#EXP-1405, the `live` ticket claim): deploy the relay BEFORE web, and roll web
back with any relay rollback. Web mints `live` viewer tickets for idle
sessions; a relay that ignores the claim shows those sessions as Ended.

## What runs when (2026-10-26)

| When | Workflow | What |
|---|---|---|
| pull request | `test.yml` (required check `gate`) | typecheck, web unit tests (4 shards), migrations from scratch + drift + custom triggers, desktop `cargo check` (only when the PR touches it), the eUI conformance pass (`exponential-ui.yml` with `conformance_only`: each renderer's conformance runner + `conformant`, only when the PR touches eUI) |
| master push | `build-web.yml`, `build-steer-relay.yml` | the ghcr images the deploys pull |
| release tags (`desktop-v*`, `android-v*`, `ios-v*`, `cli-v*`, `eui-v*`, `etalk-v*`, `epilot-v*`, `enet-v*`, `v*.*.*`) + `workflow_dispatch` | `release-suite.yml` (once per commit) | everything heavy: `exponential-ui.yml` in full (renderer suites, conformance, parity, guides, size, bench, Simulator + emulator), `test-ios.yml`, Android unit + lint + licences, desktop `cargo test` + licences, marketing + styleguide builds + `shots:check`, package + bun suites, real-DB suites, the feedback-widget Playwright e2e on the compose stack, push-relay image smoke build |
| release tags | `build-*.yml`, `release-eui.yml` | the release artifacts |
| by hand | `exponential-ui-fuzz.yml`, `record-compose-fixtures.yml`, `bump-versions.yml` | fuzzing, re-recording the Compose fixtures, the version-bump PR |

Bench timings never fail (warnings only); counts and bytes still gate.

## Release secrets and variables (EXP-1430)

The shared actions behind these workflows live in `../actions/` (see its
README; Niach/spezl calls the same ones). Every signing/upload step is
GATED: a missing secret = a `::warning::` and the unsigned/un-uploaded
fallback, never a red run, except where noted. "Person" = a human has to
create it in the vendor's console and paste it into the repo's Actions
secrets (the values live in KeePass, never in the repo). spezl needs its own
copies in Niach/spezl; the Apple ones can be the same values (same team).

| Name | Kind | Used by | What | Who provides |
|---|---|---|---|---|
| `MACOS_CERT_P12`, `MACOS_CERT_PASSWORD` | secret | `macos-app` | Developer ID Application cert (base64 .p12) | person (Apple Developer) |
| `NOTARY_KEY_ID`, `NOTARY_ISSUER_ID`, `NOTARY_KEY` | secret | `macos-app` | App Store Connect API key for notarytool (`NOTARY_KEY` = base64 .p8) | person (App Store Connect) |
| `IOS_DIST_CERT_P12`, `IOS_DIST_CERT_PASSWORD` | secret | `apple-signing` | Apple Development + Distribution certs (base64 .p12) | person |
| `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_CONTENT` | secret | `build-ios.yml`, `testflight-upload` | App Store Connect API key (profiles + TestFlight upload) | person |
| `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET` | secret | `windows-sign` | an Entra app registration with the "Artifact Signing Certificate Profile Signer" role | person (Azure; identity validation first). A PARTIAL set fails the Windows leg |
| `WINDOWS_SIGNING_ENDPOINT`, `WINDOWS_SIGNING_ACCOUNT`, `WINDOWS_SIGNING_PROFILE` | variable | `windows-sign` | Artifact Signing region endpoint, account, certificate profile | person |
| `UPDATE_SIGNING_KEY` | secret | `updater-feed` | minisign secret key (`minisign -G -W`, base64 of the file) signing `SHA256SUMS.txt` | person (generate once, keep in KeePass) |
| `UPDATE_SIGNING_PUBKEY` | variable | `updater-feed`, the desktop + CLI builds (`EXP_UPDATE_PUBKEY`) | the public key; compiled in, every update then MUST be signed. Set without the secret = the release FAILS | person (second line of the .pub) |
| `ANDROID_KEYSTORE_B64`, `RELEASE_STORE_PASSWORD`, `RELEASE_KEY_ALIAS`, `RELEASE_KEY_PASSWORD` | secret | `build-android.yml` | Play upload keystore | person |
| `PLAY_SERVICE_ACCOUNT_JSON` | secret | `play-upload` | Google Cloud service-account key, invited in Play Console as Release manager on the app | person. The Play app record + first upload must exist (the API cannot create apps) |
| `RELEASE_BOT_TOKEN` | secret | `bump-versions.yml` | token for the bump PR so `gate` runs on it (GITHUB_TOKEN PRs trigger no workflows) | person (optional) |
| `RUNNER_STATUS_TOKEN` | secret | `pick-runner.yml` | lists the self-hosted runners (Administration: read) | person (optional) |
| `NPM_TOKEN`, `CARGO_REGISTRY_TOKEN`, `MAVEN_CENTRAL_USERNAME`, `MAVEN_CENTRAL_PASSWORD`, `SIGNING_KEY`, `SIGNING_PASSWORD`, `SWIFT_DIST_TOKEN` | secret | `release-eui.yml` | Exponential UI package publishing | person |

Turning on the signed updater feed: `minisign -G -W -p update.pub -s update.key`,
`UPDATE_SIGNING_KEY` = `base64 < update.key`, `UPDATE_SIGNING_PUBKEY` = line 2
of `update.pub`, then the next `desktop-v*` / `cli-v*` tag ships pinned
clients. Rotation needs one transition release (`../actions/updater-feed/README.md`).

Version bumps: Actions → `bump-versions` → pick the products; it opens a PR
(config `.github/release-versions.json`), a person tags after the merge.
Locally: `bun run bump:versions -- --dry-run desktop=patch android=patch ios=patch`.
