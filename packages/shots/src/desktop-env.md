# Desktop app: capture env vars

DEV-ONLY environment overrides the gpui desktop app (`apps/desktop`) reads at
launch. They exist so an external script can start the app straight onto one
surface, at a fixed size, against a throwaway data dir — no synthetic input.
All are read with `std::env::var` and fall back SILENTLY when unset or
unparseable (dev-only: never surfaced as a user-facing error). None of them
are documented for users.

## Session + instance

| Var | Values | Effect |
| --- | --- | --- |
| `EXP_INSTANCE_URL` | absolute URL | Retarget the instance without a recompile (e.g. `http://localhost:3000`). |
| `EXP_DEV_SERVER` | instance URL | With `EXP_DEV_TOKEN`: inject a signed-in session at boot instead of showing login. Both must be set. |
| `EXP_DEV_TOKEN` | session/API token | See above. A token that does not resolve falls back to the login screen. |
| `EXP_DATA_DIR` | absolute path | **New.** Move the whole app data dir (`accounts.json`, per-account sync SQLite, `settings.json`). `EXP_DEV_SERVER`+`EXP_DEV_TOKEN` PERSIST the injected account, so a capture run without this rewrites the developer's real signed-in state. Always set it for captures. |
| `EXP_SKIP_ONBOARDING` | `1` | Never render the first-run wizard. |

## Where the app opens

| Var | Values | Effect |
| --- | --- | --- |
| `EXP_DEV_TEAM` | team uuid | Pre-select the team (wins over the persisted last-team/board pair). |
| `EXP_DEV_SCREEN` | `settings` \| `account` (= settings) \| `actions` \| `getting-started` \| `issue:<issue-uuid>` \| `pr:<issue-uuid>` \| `support:<thread-uuid>` | Pre-route the first screen. **New:** the `pr:` and `support:` arms. `pr:` is keyed by the ISSUE whose linked PR the diff shows (the Reviews rows open it the same way), `support:` by the support thread id. Unset = the rail tool's own center content. |
| `EXP_DEV_TOOL` | `inbox` \| `my-issues` \| `board` (also `board-issues`, `issues`) \| `reviews` \| `support` \| `files` \| `source-control` | **New.** Pre-select the rail tool window. Default `board`. `my-issues` selects the Inbox tool AND seeds its My Issues tab. |
| `EXP_DEV_INBOX_TAB` | `inbox` \| `my-issues` | **New.** The Inbox tool window's active tab. Default `inbox`; wins over the `my-issues` seed above. |
| `EXP_DEV_SETTINGS` | `general` \| `members` \| `labels` \| `statuses` \| `storage` \| `archived-boards` \| `repositories` \| `tools` \| `agents` \| `local-repos` \| `account` \| `notifications` \| `api-keys` \| `about` \| `board:<board-uuid>` | **New.** Which settings section is selected. Default `general`. This only PICKS the section — `EXP_DEV_SCREEN=settings` is what opens the settings screen. A section the signed-in user cannot see (owner-only panes for a member) clamps to the fallback pane. There is no `widget` section on desktop (web-only). |

## Window

| Var | Values | Effect |
| --- | --- | --- |
| `EXP_WINDOW_SIZE` | `<width>x<height>`, e.g. `1440x900` | **New.** Pin the launch size — logical pixels, clamped up to the 800x600 floor. Wins over BOTH the persisted last-used size and the 1280x820 default, and suppresses persistence, so a capture run never rewrites the developer's remembered size. Unparseable = ignored. |
| `EXP_WINDOWS` | `1`..`4` | Open N shell windows at startup. |

## Other dev hooks (not needed for captures)

`EXP_DEV_BOARD=1` (debug board tab in the center dock), `EXP_DEV_CREATE_DIALOG=1`
(open the create dialog at boot), `EXP_DEV_OPEN_SHELL=1` (open the terminal
dock), `EXP_SYNC_LOG` (sync client logging), `EXP_UPDATE_API`.

## Example: capture the Reviews view

```sh
EXP_INSTANCE_URL=http://localhost:3000 \
EXP_DEV_SERVER=http://localhost:3000 \
EXP_DEV_TOKEN="$EXP_SHOTS_TOKEN" \
EXP_DATA_DIR=/tmp/exp-shots/reviews \
EXP_SKIP_ONBOARDING=1 \
EXP_WINDOW_SIZE=1440x900 \
EXP_DEV_TEAM="$TEAM_ID" \
EXP_DEV_TOOL=reviews \
  ./apps/desktop/target/debug/exp-desktop   # or the built .app's binary
```

A per-view run only changes the last few lines, e.g. the API-keys settings pane:

```sh
… EXP_DEV_SCREEN=settings EXP_DEV_SETTINGS=api-keys …
```

or one PR diff (pair it with `EXP_DEV_TOOL=reviews` so the rail matches the
center view):

```sh
… EXP_DEV_TOOL=reviews EXP_DEV_SCREEN=pr:$ISSUE_ID …
```

Give every view its own `EXP_DATA_DIR` (or wipe it between runs) so nothing
carries over from the previous capture.
