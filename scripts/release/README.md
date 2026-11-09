# Release scripts

The release shepherd's deploy tooling: web deploys staging-first, client adverts, the eUI
tag, Play tracks. They drive Coolify (`coolify` CLI on a LAN machine), `ssh` to the deploy
host, `gh` and `git`; nothing here holds a secret or a host.

## Config (local, never committed)

`~/.config/exponential/release.env` (or `$EXP_RELEASE_ENV`), sourced at runtime:

| key | what |
|---|---|
| `EXP_SSH` | `user@host` of the deploy host |
| `EXP_APP_PROD`, `EXP_APP_STAGING` | Coolify uuids of the web apps |
| `EXP_APP_PUSH_RELAY` | Coolify uuid of the push relay |
| `EXP_DB_PROD`, `EXP_DB_STAGING` | Coolify uuids of the Postgres containers |
| `PLAY_SERVICE_ACCOUNT_JSON` | path of the Play publisher key (`play.py`) |
| `EXP_PROD_URL`, `EXP_STAGING_URL`, `EXP_PUSH_URL`, `EXP_WEB_IMAGE`, `EXP_STATE` | optional overrides |

## Scripts

| script | does |
|---|---|
| `webdeploy.sh <sha7>` | device-shares on both DBs, staging, boot-log check (holds prod on any error), prod; `STAGING_ONLY=1` for a soak, `PRE_PROD_HOOK=…` |
| `land-deploy.sh <PR> <mig 0/1> <relay 0/1>` | waits for `(#PR)` on master, then the above (a migrations-only suite first when asked), then the push relay; one deploy at a time; `WAIT_EXACT=1` = that PR's own image only |
| `hotfix-web.sh` | after a `release-suite -f only=migrations` dispatch: migrations job, image, `webdeploy.sh` |
| `advert.sh KEY=VER…` | sets prod's `CLIENT_LATEST_VERSION_*`, redeploys prod's current image |
| `eui-tag.sh <ver> <PR>` | full release suite on master after the PR; tags `eui-v<ver>` only if every eUI leg is green |
| `shares.sh` · `deploy.sh <uuid> <url>` | building blocks |
| `play.py <pkg> show / promote / to` | Play tracks (`to` never targets production) |

**Pins:** `tagkeep.sh` records each web app's image tag before a deploy and settles it after:
an app that was on `latest` goes back to `latest`; a **pinned** app (`sha-*`, e.g. prod during
a migration soak) stays pinned, on the image now running. Nothing here sets `latest` on a
pinned app. `test-tagkeep.sh` checks that in a dry run (CI: `test.yml` › `release-scripts`).

GitHub's API quota is shared by every session on one login: the scripts watch landings with
`git fetch` and images in the registry, and poll `gh` at most every 5 minutes.
