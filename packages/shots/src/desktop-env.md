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
| `EXP_DEV_ONBOARDING` | `choice` \| `create` \| `join` \| `invite` \| `devices` | **New.** Which page of the wizard to open. `choice`/`create`/`join` are the Team step's sub-pages and need an account with no team (the seed's newcomer). `invite` and `devices` (EXP-725) are the post-board tail: they force the wizard past the Board step and resolve their team from the account's first one, so they need the STARTER identity (one board-less team) — `devices` additionally presets the invite step as done. All of them need NO `EXP_SKIP_ONBOARDING`; with it, or with the wrong identity, the shell renders instead. |

## Where the app opens

| Var | Values | Effect |
| --- | --- | --- |
| `EXP_DEV_TEAM` | team uuid | Pre-select the team (wins over the persisted last-team/board pair). |
| `EXP_DEV_SCREEN` | `settings` \| `account` (= settings) \| `devices` \| `actions` \| `action:<action-uuid>` \| `home` \| `chats` \| **`board-issues`** \| **`inbox`** (Home + the Inbox popover) \| **`inbox-my-issues`** (the active board on its My issues pill) \| `needs-you` \| **`files`** \| **`source-control`** \| `drafts` \| `draft` \| `issue:<issue-uuid>` \| `chat:<root session uuid>` \| `session:<session uuid>` \| `agent:<agent root session uuid>` \| `specimen:<entry id>` | Pre-route the first screen. **EXP-851:** the rail's tool windows are SCREENS — `board-issues` (also `board` / `issues`) is the active board's full-width list, `inbox` / `inbox-my-issues` the two tabs of the personal list, and `files` / `source-control` the remaining two. A board list takes the window's ACTIVE board, so `EXP_DEV_BOARD_ID` still picks which one. **EXP-1294:** `home` is HOME, the composer page every client lands on (it replaced `chat`; there is no seeded `chat?…` spelling any more — a seeded composer is the `run:` dialog below), and `chats` the Chats page (bands Needs you / Running / Recent of the active team). The three `sessions` drives are keyed by the `sessions` ROW id: `chat:` opens a chat root (kind `thread`) in its tab, `session:` any session of a tree — it opens INSIDE its root's tab (Back returns to the chat), with no engine in this process the transcript dials the relay as a viewer, so it shows Reconnecting unless a machine is publishing that session (the `steering` capture pairs it with `bun run screenshots:desktop`) — and `agent:` an agent root's channel. There is NO `new-agent` screen value (`parse_dev_screen` has no such arm): the New agent page is the settings section `EXP_DEV_SCREEN=settings EXP_DEV_SETTINGS=new-agent` (EXP-1319). `reviews`, `chat`, `chat?…`, `support:` and `issue:<uuid>?face=…` are gone with their surfaces (an issue spec is the bare id). **EXP-1259 → #EXP-1779:** `specimen:<id>` draws ONE dev-only specimen (`crates/ui/src/specimens/entries`, any registered id) full-window, for the views whose states never show together in the app (`feedback-states`); an unknown id = no pre-route. Unset = Home. |
| `EXP_DEV_TOOL` | `inbox` \| `my-issues` \| `board` (also `board-issues`, `issues`) \| `agent` (also `sessions`) \| `files` \| `source-control` | **EXP-851: a compatibility alias.** There is no tool column anymore — each value names the SCREEN the tool window became (`navigation::dev_tool_screen`; `agent` / `sessions` = Home). Alone it opens that screen, exactly like the matching `EXP_DEV_SCREEN` value; `EXP_DEV_SCREEN` always wins. Beside a DETAIL screen that carries a list (`issue:` / `chat:` / `session:`) the Inbox values mean the Inbox second sidebar beside it — the pair the app itself produces when a row is clicked in the Inbox. EXP-1294 retired `history` (Agent › Recent) and `reviews`. |
| `EXP_DEV_BOARD_ID` | board uuid | **New.** Pre-select the board, for the cases the last-visited one is the wrong one (the empty-board view). `EXP_DEV_BOARD=1` was already taken by an unrelated debug tab, hence the `_ID`. Only assigned when nothing else already picked a board. |
| `EXP_DEV_DIALOG` | see below | Open ONE dialog, once, from the render path after the state it needs resolves. Every desktop dialog is its own OS window centred over the opener, so it lands inside the main window's rect. |
| `EXP_DEV_SETTINGS` | `general` \| `members` \| `issues` \| `labels` \| `statuses` \| `storage` \| `archived-boards` \| `repositories` \| `tools` \| `agent-clis` \| `agents` \| `agent:<agent root session uuid>` \| `new-agent` \| `local-repos` \| `sessions` \| `account` \| `notifications` \| `api-keys` \| `mcp` \| `mcp-activity` \| `about` \| `board:<board-uuid>` | Which settings section is selected (parsed by `sidebar.rs` `parse_dev_settings_section`; `agents` resolves to the first agent in `settings/mod.rs` once the sessions shape synced). Default `general`. This only PICKS the section — `EXP_DEV_SCREEN=settings` is what opens the settings screen. **EXP-1294:** `agent-clis` is the per-device Coding CLIs pane (it was `agents`); `agents` is now the team's Agents group (its first agent, or empty), `agent:<id>` one agent's page, `new-agent` (EXP-1319, owner-only) the New agent page. A section the signed-in user cannot see (owner-only panes for a member) clamps to the fallback pane. **EXP-1558:** `widget` is the read-only Widget pane (configuration stays web-only). |

### `EXP_DEV_DIALOG` specs

Parsed by `screens::parse_dev_dialog`; an unrecognised spec logs once and opens
nothing (and releases the ready probe, so the run fails fast instead of hanging).

Bare: `search`, `create-board`, `create-team`, `join-team`, `add-server`,
`getting-started` (the rail entry's Getting started dialog, active team;
EXP-1259), `app-consent` (EXP-1558: THE consent window an ungranted typed
action raises, for the natives' `sg_app-consent` call; pair it with
`EXP_DEV_SCREEN=app:working-hours`). EXP-1319 retired `create-agent` (New agent is the settings page
`EXP_DEV_SETTINGS=new-agent` now). EXP-1294 retired `tidy-up` (the builtin actions are gone) and the
`trigger-new` / `trigger-edit` forms (triggers live on agents now).

With an argument: `join-team:<invite-token>`, `action-editor:<action-uuid>`
(an alias that opens the action's page), `device-settings:<device-uuid>`,
`duplicate-picker:<issue-uuid>`, `run:<issue-uuid>[,<issue-uuid>…]` (the
composer dialog on those issues — the seeded composer).

Pair the spec with whatever should be BEHIND it via a second var — the catalog
does this with `EXP_DEV_TOOL=board` under the search palette (EXP-851: that now
opens the board's full-width list screen), `EXP_DEV_SCREEN=devices` under the
device dialogs.

## Pre-seeded state on the opened screen

| Var | Values | Effect |
| --- | --- | --- |
| `EXP_DEV_SELECT` | `APP-6,APP-7,APP-9` | **New.** Pre-select those issues on the board so the bulk-action bar renders. Identifiers, not uuids. Applied once, and only when EVERY named row has synced — a partial selection would photograph a different bar. |
| `EXP_DEV_SEARCH_QUERY` | free text | **New.** Open the search palette with this query already typed and its results resolved. Pairs with `EXP_DEV_DIALOG=search`. |
| `EXP_DEV_SPEAKING` | `<session uuid>\|<en\|de>\|<reply body>` | **New (NEW-74).** Draw that session's Speaking banner (voice.json) as if the reply were being read aloud: the title is `spokenText(body, maxChars, language)`'s first sentence. No speech worker, no GPU. Pair it with `EXP_DEV_SCREEN=agent:<root>` (or `chat:`/`session:`) on the same session. |
| `EXP_DEV_PTT_OVERLAY` | `<phase>\|<line>\|<agent>` | **New (NEW-54).** Open the push-to-talk overlay window (voice.json `pttOverlay`: THE capsule, never focused, at the bottom centre of the display) in that phase (`listening`, `heard`, `thinking`, `speaking`, `spoken`, `reply`, `muted`, `notSent`, `failed`) with that line and the agent's name on the pill. No microphone, speech worker or sign-in needed. |
| `EXP_DEV_WAKE_LISTENING` | `1` | **New (NEW-97).** Read as if the speech worker held the wake word's microphone open (voice.json `trayIcon` `listening`): the tray dot, the rail voice button's dot and the Wake word row's Listening. No microphone or speech worker. The rail button itself also needs this device as the member's voice device with the wake word on, and no tray host (Xvfb has none). |
| `EXP_DEV_CHAT_CHILD` | `results` | Open a `chat:` screen on its Results tab instead of Conversation (`session_chat.rs`; results-tab.json). |
| `EXP_DEV_INBOX_OPEN` | `1` | **New (#FEED-191).** Open the rail header's Inbox popover on launch, over whatever the window opens on. `EXP_DEV_TOOL=inbox` does it over Home. |
| `EXP_DEV_SCROLL_TO` | `script-card` | **New (EXP-1259).** Park a `session:` screen's transcript (always the full transcript since EXP-1294) with its FIRST code-mode Script card near the top of the viewport (one row of lead-in above it) instead of on the tail (`screens::dev_scroll_to_script_card`). Re-applied only while that row's index moves (the replay landing rows above it), so a wheel is never fought. The `script-card` capture pairs it with the `steering` drive. **EXP-1558:** the parked card is then CENTRED in the viewport (web's `centreCard`); `work-card` and `comment-call-card` park on those cards the same way, and `end` parks the issue page (`issue-comments`) and the settings column (`settings-assistant-meeting`) at their end, re-applied while the content still grows (`screens::dev_park_at_end`). |
| `EXP_DEV_COMPOSER_MENU` | `implement-issue` \| `dialog-implement-issue` \| `temporary` | **New (EXP-1259).** Open Home's "+" menu ONCE, as a click would, with its first row selected, as a hover would, so the Implement issue submenu (THE search list) is open beside it (`screens::take_dev_composer_menu`, `controls::PointerMenu::dev_open`). It waits for the full menu (the launch cluster), synced shapes and a laid-out +; after that it closes like any menu. Pairs with `EXP_DEV_SCREEN=home` (the `composer-menu` capture). **N-6:** `dialog-implement-issue` is the same for the composer DIALOG's "+" — pair it with `EXP_DEV_DIALOG=run:<issue>`; the dialog's menu opens in a menu window over the dialog (`crates/ui/src/menu_window.rs`), which is what the `composer-dialog-menu` capture photographs. **EXP-1551:** `temporary` starts Home's agent row on Temporary session and opens the "+" menu at its ROOT with no row lit (`PointerMenu::dev_steps(0)`): the one state all four clients photograph for `composer-menu`. |
| `EXP_DEV_COMPOSER_TEXT` | free text | **New (EXP-1557).** Type this text into Home's composer ONCE, as keystrokes would, on the first page composer with a team and synced shapes (`screens::take_dev_composer_text`, `MentionInput::dev_type`): `Explain #APP` opens THE `#` search list at the caret, its input prefilled with `APP`. Pairs with `EXP_DEV_SCREEN=chat` (the `search-list` capture). **EXP-1558:** a `#` waits for the composer's first paint, so the list opens at the caret; `/` opens the prompts menu (`composer-slash-menu`); a session view (`chat:`/`session:`) types it into its own composer once its row synced (`session-slash-menu`). |
| `EXP_DEV_COMPOSER_ISSUES` | `<issue-uuid>,<issue-uuid>` | **New (EXP-1649).** Seed Home's composer INLINE with these issues once (the first page composer with a team and synced shapes): web's `/agent?issues=` deep link, the `chat-issues` capture (`screens::take_dev_composer_issues`). A ▶ seed opens the composer dialog instead (EXP-1037). |
| `EXP_DEV_TYPEAHEAD_QUERY` | free text | **New (EXP-1558).** Type this into the first `#` search list's OWN input once it opens: with `EXP_DEV_COMPOSER_TEXT=Explain #` the composer keeps `Explain #` and the list reads `APP`, web's `typeIssueRef` state (`screens::take_dev_typeahead_query`). |
| `EXP_DEV_DISK_FREE` | bytes | **New (EXP-1558).** Settings → Worktrees reads this headroom (against a 500 GB disk) instead of the capture Mac's own (`local_repos::dev_disk_free`): `6657199309` = the device-doctor fixture's low-disk 6.2 GB. |
| `EXP_DEV_DOCTOR_CASE` | case name | **New (EXP-1654).** THIS device's readiness block (Settings → Tools, the first-run devices step) draws that case of `packages/domain-contract/fixtures/device-doctor.json` instead of the capture Mac's own doctor run (`device_readiness::dev_doctor_case`). Exact `name` match; unknown = the live block. `settings-tools` pins the low-disk case, the machine `settings-worktrees` photographs. |
| `EXP_DEV_SPECIMEN_ONLY` | `1` | **New (EXP-1558).** `specimen:device-readiness` draws only its web specimen (the low-disk case + the composer row), not every fixture case after it. |
| `EXP_DEV_OPEN_SHELL` | `1` | Open one shell tab (a fullscreen terminal screen + its session-bar tab). |
| `EXP_DEV_SHELL_CWD` | absolute path | Where that shell starts. Default `$HOME`, whose directory name becomes the tab title — i.e. the developer's account name. The lane sets it to `--repos-root` so the store stays username-free (EXP-651). |
| `EXP_DEV_AGENT_ACCOUNT` | email | Settings → Agents renders the LOCAL install's own sign-in and usage rows. With this set the pane never probes the machine: it stands demo accounts (claude + codex as this address) and pinned usage windows in place of the operator's real ones (EXP-698/733). The numbers match the shots stub's synced device row (`apps/web/scripts/screenshot-demo.ts` `DEMO_AGENT_STATUS`). EXP-951: the same demo payload is what `devices.register` and the heartbeat REPORT for this machine, so the lane sets it on EVERY launch — the capture host's row on the Agents screen never carries the operator's logins. |
| `EXP_DEV_NO_REGISTER` | `1` | **New (EXP-1654).** Never call `devices.register` for this machine (`steer_wiring::register_device`), so the capture Mac never joins the demo fleet it photographs: the Devices page and the Add-device dialog show the seeded machines only, as on web and the natives. Heartbeats only update an existing row. The lane sets it on EVERY launch. |
| `EXP_DEV_PUBLIC_URL` | absolute URL | **New (EXP-1654).** The instance origin the app PRINTS for copying (Settings → MCP's endpoint, the Add-device server one-liner) in place of the account's instance URL (`queries::display_instance_url`); requests still go to the instance. The lane passes the web lane's origin (`CAPTURE_BASE_URL`, else `SHOTS_PROXY_URL`, else `https://localhost:3000`), so desktop never photographs the vite port. |

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

## Example: capture the Chats page

```sh
EXP_INSTANCE_URL=http://localhost:3000 \
EXP_DEV_SERVER=http://localhost:3000 \
EXP_DEV_TOKEN="$EXP_SHOTS_TOKEN" \
EXP_DATA_DIR=/tmp/exp-shots/chats \
EXP_SKIP_ONBOARDING=1 \
EXP_WINDOW_SIZE=1440x900 \
EXP_DEV_TEAM="$TEAM_ID" \
EXP_DEV_READY_FILE=/tmp/exp-shots/ready.json \
EXP_DEV_SCREEN=chats \
  ./apps/desktop/target/debug/exp-desktop   # or the built .app's binary
```

A per-view run only changes the last few lines, e.g. the API-keys settings pane:

```sh
… EXP_DEV_SCREEN=settings EXP_DEV_SETTINGS=api-keys …
```

or one chat (its Conversation, in its tab):

```sh
… EXP_DEV_SCREEN=chat:$ROOT_SESSION_ID …
```

or one session of it (the `steering` view — the relay stub has to be
publishing that session, otherwise the screen renders Reconnecting):

```sh
… EXP_DEV_SCREEN=session:$SESSION_ID …
```

Give every view its own `EXP_DATA_DIR` (or wipe it between runs) so nothing
carries over from the previous capture.
