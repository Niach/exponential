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
drifted. Review the pin list by hand too. During the
rolling deploy, old pods still register without writing first-seen (a short
window: the next register on a new pod writes it).

Steer relay order (#EXP-1415): deploy the relay BEFORE web, and roll web
back with any relay rollback. Web mints `live` viewer tickets for idle
sessions; a relay that ignores the claim shows those sessions as Ended.

## What runs when (2026-10-26)

| When | Workflow | What |
|---|---|---|
| pull request | `test.yml` (required check `gate`) | typecheck, web unit tests (4 shards), migrations from scratch + drift + custom triggers, desktop `cargo check` (only when the PR touches it) |
| master push | `build-web.yml`, `build-steer-relay.yml` | the ghcr images the deploys pull |
| release tags (`desktop-v*`, `android-v*`, `ios-v*`, `cli-v*`, `ui-v*`, `v*.*.*`) + `workflow_dispatch` | `release-suite.yml` (once per commit) | everything heavy: `exponential-ui.yml` (renderers, conformance, bench), `test-ios.yml`, Android unit + lint + licences, desktop `cargo test` + licences, marketing + styleguide builds + `shots:check`, package + bun suites, real-DB suites, the feedback-widget Playwright e2e on the compose stack, push-relay image smoke build |
| release tags | `build-*.yml`, `release-ui.yml` | the release artifacts |
| by hand | `exponential-ui-fuzz.yml`, `record-compose-fixtures.yml` | fuzzing, re-recording the Compose fixtures |

Bench timings never fail (warnings only); counts and bytes still gate.
