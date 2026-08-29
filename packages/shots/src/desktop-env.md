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
| `EXP_SKIP_ONBOARDING` | `1` | Never render the first-run wizard. Every capture sets it EXCEPT the `onboarding` drives, which are the wizard. |
| `EXP_DEV_LOGIN` | `cloud` \| `self-hosted` | **New.** Which state the signed-out login card starts in: the cloud one (OIDC buttons over the password form) or the self-hosted one (the Server URL field revealed). Anything else is ignored. Only meaningful with NO injected session. |
| `EXP_DEV_ONBOARDING` | `choice` \| `create` \| `join` | **New.** Which sub-page of the wizard's Team step to open. Needs an account with no team (the seed's newcomer) and NO `EXP_SKIP_ONBOARDING` — with either missing the shell renders instead. |

## Where the app opens

| Var | Values | Effect |
| --- | --- | --- |
| `EXP_DEV_TEAM` | team uuid | Pre-select the team (wins over the persisted last-team/board pair). |
| `EXP_DEV_SCREEN` | `settings` \| `account` (= settings) \| `devices` \| `actions` \| `automations` \| `getting-started` \| `issue:<issue-uuid>` \| `pr:<issue-uuid>` \| `support:<thread-uuid>` | Pre-route the first screen. **New:** the `pr:` and `support:` arms. `pr:` is keyed by the ISSUE whose linked PR the diff shows (the Reviews rows open it the same way), `support:` by the support thread id. Unset = the rail tool's own center content. |
| `EXP_DEV_TOOL` | `inbox` \| `my-issues` \| `board` (also `board-issues`, `issues`) \| `reviews` \| `support` \| `files` \| `source-control` | **New.** Pre-select the rail tool window. Default `board`. `my-issues` selects the Inbox tool AND seeds its My Issues tab. |
| `EXP_DEV_INBOX_TAB` | `inbox` \| `my-issues` | **New.** The Inbox tool window's active tab. Default `inbox`; wins over the `my-issues` seed above. |
| `EXP_DEV_BOARD_ID` | board uuid | **New.** Pre-select the board, for the cases the last-visited one is the wrong one (the empty-board view). `EXP_DEV_BOARD=1` was already taken by an unrelated debug tab, hence the `_ID`. Only assigned when nothing else already picked a board. |
| `EXP_DEV_DIALOG` | see below | Open ONE dialog, once, from the render path after the state it needs resolves. Every desktop dialog is its own OS window centred over the opener, so it lands inside the main window's rect. |
| `EXP_DEV_SETTINGS` | `general` \| `members` \| `labels` \| `statuses` \| `storage` \| `archived-boards` \| `repositories` \| `tools` \| `agents` \| `local-repos` \| `account` \| `notifications` \| `api-keys` \| `about` \| `board:<board-uuid>` | **New.** Which settings section is selected. Default `general`. This only PICKS the section — `EXP_DEV_SCREEN=settings` is what opens the settings screen. A section the signed-in user cannot see (owner-only panes for a member) clamps to the fallback pane. There is no `widget` section on desktop (web-only). |

### `EXP_DEV_DIALOG` specs

Parsed by `screens::parse_dev_dialog`; an unrecognised spec logs once and opens
nothing (and releases the ready probe, so the run fails fast instead of hanging).

Bare: `create-issue`, `search`, `start-coding`, `start-coding-actions`,
`start-coding-chat`, `create-action`, `automation-new`, `create-board`,
`create-team`, `join-team`, `add-server`.

With an argument: `join-team:<invite-token>`, `action-editor:<action-uuid>`,
`automation-edit:<automation-uuid>`, `device-settings:<device-uuid>`,
`duplicate-picker:<issue-uuid>`.

Pair the spec with whatever should be BEHIND it via a second var — the catalog
does this with `EXP_DEV_TOOL=board` under the search palette, `EXP_DEV_SCREEN=actions`
under the launcher.

## Pre-seeded state on the opened screen

| Var | Values | Effect |
| --- | --- | --- |
| `EXP_DEV_FILTER` | `1` | **New.** Render the board's filter popover already open. (Before EXP-642 this set gpui-component's `default_open`, which marks the popover open without registering it — nothing appeared. It now opens for real.) |
| `EXP_DEV_SELECT` | `APP-11,APP-13,APP-10` | **New.** Pre-select those issues on the board so the bulk-action bar renders. Identifiers, not uuids. Applied once, and only when EVERY named row has synced — a partial selection would photograph a different bar. |
| `EXP_DEV_SEARCH_QUERY` | free text | **New.** Open the search palette with this query already typed and its results resolved. Pairs with `EXP_DEV_DIALOG=search`. |
| `EXP_DEV_GETTING_STARTED_TAB` | `first-steps` \| `suggestions` | The Getting started screen's active tab (EXP-686 — the action suggestion seeds moved there from the Actions screen, which no longer has tabs; Devices and Automations are their own `EXP_DEV_SCREEN` values). |
| `EXP_DEV_OPEN_SHELL` | `1` | Open the terminal dock. |
| `EXP_DEV_SHELL_CWD` | absolute path | Where that shell starts. Default `$HOME`, whose directory name becomes the tab title — i.e. the developer's account name. The lane sets it to `--repos-root` so the store stays username-free (EXP-651). |

## The ready handshake

| Var | Values | Effect |
| --- | --- | --- |
| `EXP_DEV_READY_FILE` | absolute path | **New (EXP-633).** Poll own state every 250ms and write `{"ready_at_ms","shapes","dialog"}` to this path ONCE everything the capture depends on has settled: a window exists, the session is where the run wants it (signed-in: `Synced` + every shape past its first `up-to-date`; signed-out: `SignedOut` with no auth-config fetch in flight), and any `EXP_DEV_DIALOG` has opened. Until then it prints what it is blocked on every 5s, whenever that reason changes. A no-op when unset. |

This replaced a fixed sleep in the capturer. If a view times out, run the app by
hand with the SAME env plus `EXP_DEV_READY_FILE=/tmp/ready.json` and read the
`waiting: …` lines — they name the shapes that never went live.

## Window

| Var | Values | Effect |
| --- | --- | --- |
| `EXP_WINDOW_SIZE` | `<width>x<height>`, e.g. `1440x900` | **New.** Pin the launch size — logical pixels, clamped up to the 800x600 floor. Wins over BOTH the persisted last-used size and the 1280x820 default, and suppresses persistence, so a capture run never rewrites the developer's remembered size. Unparseable = ignored. |
| `EXP_WINDOWS` | `1`..`4` | Open N shell windows at startup. |

## Other dev hooks (not needed for captures)

`EXP_DEV_BOARD=1` (debug board tab in the center dock — NOT `EXP_DEV_BOARD_ID`),
`EXP_DEV_CREATE_DIALOG=1` (open the create dialog at boot), `EXP_SYNC_LOG` (sync
client logging), `EXP_UPDATE_API`.

## Example: capture the Reviews view

```sh
EXP_INSTANCE_URL=http://localhost:3000 \
EXP_DEV_SERVER=http://localhost:3000 \
EXP_DEV_TOKEN="$EXP_SHOTS_TOKEN" \
EXP_DATA_DIR=/tmp/exp-shots/reviews \
EXP_SKIP_ONBOARDING=1 \
EXP_WINDOW_SIZE=1440x900 \
EXP_DEV_TEAM="$TEAM_ID" \
EXP_DEV_READY_FILE=/tmp/exp-shots/ready.json \
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
