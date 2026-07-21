# EXP-177 — Comprehensive codebase review (2026-07-21)

Second full-codebase review (the first was 2026-07-10 → board `REV`). It answers the six questions posed in EXP-177: logic inconsistencies/gaps, whole-flow UX, cross-platform consistency, stale legacy code/comments, compute/scale hazards toward **1,000 concurrent users on one Hetzner CX33**, and whether account deletion truly removes everything.

## How this review was run

A multi-agent workflow fanned out **14 territory-scoped reviewers** across the whole monorepo (server auth + Electric shapes, core tRPC, platform/integration tRPC, a dedicated account-deletion deep-dive, a scaling-to-1k reviewer, a repo-wide legacy sweep, iOS/Android/desktop parity, web end-to-end UX, relays + widget, DB schema/contract drift, a cross-client behavior matrix, and marketing/docs accuracy). Every raw finding was then **deduplicated** and **adversarially verified** by an independent skeptic agent instructed to refute it; a **completeness critic** spawned follow-up reviewers for thin-coverage areas (email/digest, helpdesk, mobile-auth, push, iOS-core). All reviewers were seeded with the 108 findings from the 2026-07-10 review so nothing already fixed or dismissed was re-filed.

**Funnel:** 90 raw → 82 after dedup → **79 confirmed** + **22 from the critic round** = **101 confirmed findings** (3 refuted as false positives and dropped).

## Where the findings live

All 101 findings are filed as individual issues on a new board — **Code Review Findings (2026-07-21)**, prefix `REV2` — in the Exponential Feedback team, each with a self-contained description (evidence at `file:line`, impact, suggested fix), labeled by category, and prioritized by severity. Category labels created for this review: `logic`, `security`, `ux`, `platform-parity`, `legacy-cleanup`, `performance`, `data-deletion`.

### By severity (→ issue priority)

| Severity | Count |
| --- | --- |
| 🔴 urgent | 1 |
| 🟠 high | 12 |
| 🟡 medium | 51 |
| ⚪ low | 37 |

### By category

| Category | Label | Count |
| --- | --- | --- |
| Security | `security` | 8 |
| Scaling & performance | `performance` | 11 |
| Account-deletion completeness | `data-deletion` | 6 |
| Cross-platform parity | `platform-parity` | 16 |
| Logic & correctness | `logic` | 25 |
| UX & product flow | `ux` | 18 |
| Legacy / stale code | `legacy-cleanup` | 17 |
| **Total** | | **101** |

## Headline findings

Highest-severity items worth triaging first:

- 🔴 **[REV2-1]** (security) — iOS attaches the session bearer token to ANY markdown image URL containing "/api/", leaking it to attacker-controlled hosts
- 🟠 **[REV2-3]** (performance) — Push fan-out: timeout-less per-recipient relay fetches with unbounded concurrency can drain Bun's global fetch pool and stall Electric sync when the relay wedges
- 🟠 **[REV2-4]** (performance) — Steer relay rate limit collapses to one global 120/min bucket by default (TRUST_PROXY undocumented, valid tickets share it); web's fixed 3s redial saturates it
- 🟠 **[REV2-5]** (performance) — Board create/trash rotates all 8 board-scoped shape identities for every team member, forcing full cross-team re-syncs that Bun buffers wholly in RAM
- 🟠 **[REV2-6]** (performance) — Bun's default 256-outbound-fetch cap serializes Electric shape long-poll proxying at ~18 concurrent clients
- 🟠 **[REV2-7]** (performance) — Shape long-poll renewals re-run 24 membership queries + up to 14 DB session lookups per client per ~60s cycle, uncached and write-amplified
- 🟠 **[REV2-8]** (performance) — Missing indexes on hot FK/lookup columns: notifications.issue_id, issues.pr_url/duplicate_of_id, issue_subscribers.issue_id — cascades and PR resolution seq-scan the biggest tables
- 🟠 **[REV2-9]** (platform-parity) — Desktop coding_sessions shape omits needs_input, so the EXP-214 "Needs input" badge can never fire on desktop — even for its own sessions
- 🟠 **[REV2-2]** (logic) — Android: tapping an inbox Support group marks it read then bounces to Issues because the support-inbox pop-guard reads the pre-switch team's stale helpdesk flag
- 🟠 **[REV2-10]** (ux) — Support magic-link email failure is recorded but surfaced nowhere — reporter's only credential silently never arrives and no UI (not even admin) can see it
- 🟠 **[REV2-11]** (ux) — Self-host instructions (marketing docs + README) fail on a fresh clone: env lands where nothing reads it, wrong DB name in apps/web/.env.example, Garage secrets step missing
- 🟠 **[REV2-12]** (ux) — Auth guards redirect to login with `redirect: undefined`, dropping deep-link destinations (emailed issue links land on default team)
- 🟠 **[REV2-13]** (ux) — Support unread badge never clears from the Support inbox — support_reply notifications are only marked read via the Inbox tab

## Full index by category

### Security — 8 (`security`)

| ID | Sev | Finding |
| --- | --- | --- |
| REV2-1 | 🔴 | iOS attaches the session bearer token to ANY markdown image URL containing "/api/", leaking it to attacker-controlled hosts |
| REV2-14 | 🟡 | Email digest sweep skips the membership + board-trash scoping the notifications shape enforces — ex-members and trashed-board rows still get digested |
| REV2-15 | 🟡 | iOS and Android sign-out never revokes the Better Auth session server-side (desktop does) — a leaked bearer token survives sign-out for up to the 60-day sliding session expiry |
| REV2-16 | 🟡 | Personal expu_ API key rows survive account deletion — apikeys has no FK to users and neither deletion path cleans them up, contradicting the cascade comment |
| REV2-17 | 🟡 | Activity redactor's exact-secret layer is dead for the installation token post-EXP-73: it parses the always-bare origin URL instead of .git/exp-git-credentials |
| REV2-65 | ⚪ | Steer relay HTTP admin endpoints lack the failed-auth throttle its own comment claims (push relay has it) |
| REV2-66 | ⚪ | Creem plugin accepts unset CREEM_WEBHOOK_SECRET silently — webhook endpoint never registers, subscription events 404 with no boot warning |
| REV2-67 | ⚪ | Server-only teams.comp_tier leaks through MCP exponential_teams_get and tRPC teams.getDefault/update full-row selects |

### Scaling & performance — 11 (`performance`)

| ID | Sev | Finding |
| --- | --- | --- |
| REV2-3 | 🟠 | Push fan-out: timeout-less per-recipient relay fetches with unbounded concurrency can drain Bun's global fetch pool and stall Electric sync when the relay wedges |
| REV2-4 | 🟠 | Steer relay rate limit collapses to one global 120/min bucket by default (TRUST_PROXY undocumented, valid tickets share it); web's fixed 3s redial saturates it |
| REV2-5 | 🟠 | Board create/trash rotates all 8 board-scoped shape identities for every team member, forcing full cross-team re-syncs that Bun buffers wholly in RAM |
| REV2-6 | 🟠 | Bun's default 256-outbound-fetch cap serializes Electric shape long-poll proxying at ~18 concurrent clients |
| REV2-7 | 🟠 | Shape long-poll renewals re-run 24 membership queries + up to 14 DB session lookups per client per ~60s cycle, uncached and write-amplified |
| REV2-8 | 🟠 | Missing indexes on hot FK/lookup columns: notifications.issue_id, issues.pr_url/duplicate_of_id, issue_subscribers.issue_id — cascades and PR resolution seq-scan the biggest tables |
| REV2-38 | 🟡 | Android: no app-lifecycle gating — 14 long-poll shape loops per account keep polling while backgrounded, until process death/freeze |
| REV2-39 | 🟡 | Digest sweep fans out all user emails via unbounded Promise.all; throttle failures hit the 22h backoff and rows ≥2h old silently age past the 24h backstop |
| REV2-40 | 🟡 | Support inbox listThreads is unpaginated and fetches every public message body of the filtered thread set on every 30s poll |
| REV2-41 | 🟡 | notifications/issue_events grow unbounded: no retention sweep, full-history shape sync, unindexed fan-out dedupe probe |
| REV2-78 | ⚪ | branchDiffCache never evicts: full patch text of every branch ever viewed accumulates for the process lifetime |

### Account-deletion completeness — 6 (`data-deletion`)

| ID | Sev | Finding |
| --- | --- | --- |
| REV2-35 | 🟡 | iOS share-extension board mirror in app-group defaults is never pruned on sign-out, remove-server, or delete-account |
| REV2-36 | 🟡 | Account deletion cascades destroy teammates' work on shared-team issues and leave dangling image embeds in surviving content |
| REV2-37 | 🟡 | Account deletion leaves the deleted user's email as raw @email mention text in shared-team issues/comments — anonymizer helper exists but is never called |
| REV2-75 | ⚪ | Account deletion leaves the user's email in email_bounces, Better Auth verifications, and team_invites rows addressed to them |
| REV2-76 | ⚪ | Account deletion revokes Apple OAuth tokens but discards Google offline refresh tokens unrevoked |
| REV2-77 | ⚪ | Widget-bot users orphan permanently on team deletion: both delete endpoints refuse isAgent even at zero issues, and admins cannot see them |

### Cross-platform parity — 16 (`platform-parity`)

| ID | Sev | Finding |
| --- | --- | --- |
| REV2-9 | 🟠 | Desktop coding_sessions shape omits needs_input, so the EXP-214 "Needs input" badge can never fire on desktop — even for its own sessions |
| REV2-42 | 🟡 | Android read views never render @email mentions as name pills (contract requires it; web + desktop do) |
| REV2-43 | 🟡 | iOS 426 update gate is process-global: one outdated/misconfigured instance bricks every account in the multi-account app (parity with Android finding) |
| REV2-44 | 🟡 | Mobile login has no Create-account/Forgot-password affordance — self-hosted password instances dead-end on iOS/Android (and no reset link on desktop) |
| REV2-45 | 🟡 | Android push registration has no retry/backoff and no post-flight sign-out cleanup — iOS reconcile loop closed both gaps |
| REV2-46 | 🟡 | iOS Inbox and Agents relative timestamps render blank for synced rows (bare ISO8601DateFormatter instead of WireTimestamps.parse) |
| REV2-47 | 🟡 | Archived boards/issues hidden on Android/desktop (and partly iOS) but fully visible on web |
| REV2-48 | 🟡 | Due-date overdue/today coloring missing on web and desktop — the overdue-first sort has no visual explanation there |
| REV2-49 | 🟡 | Desktop sync drops live due_time/end_time as "stale" columns — yet its own create dialog writes them; times invisible/uneditable on desktop, CLAUDE.md denies the feature |
| REV2-79 | ⚪ | Android: synced attachment width/height never read — read-mode embedded images render unsized and jump the layout on load |
| REV2-80 | ⚪ | Desktop OAuth-exchange failures are silent: no login-view error and a stale "Waiting for your browser…" button label, unlike iOS/Android |
| REV2-81 | ⚪ | Android push taps for a non-active account drop the deep link (open app, no navigation); iOS routes them into the owning account |
| REV2-82 | ⚪ | Android inbox pins Support groups above the stream; web/iOS/desktop interleave them by latest activity |
| REV2-83 | ⚪ | Desktop @mention autocomplete matches only prefixes while web/iOS/Android match substrings |
| REV2-84 | ⚪ | My Issues diverges per client: team-scoped + filter bar on web/desktop, account-wide + unfilterable on iOS/Android |
| REV2-85 | ⚪ | Status color/glyph identity and picker ordering diverge across clients (cancelled red vs muted; Android backlog≡todo; options-array vs contract displayOrder) |

### Logic & correctness — 25 (`logic`)

| ID | Sev | Finding |
| --- | --- | --- |
| REV2-2 | 🟠 | Android: tapping an inbox Support group marks it read then bounces to Issues because the support-inbox pop-guard reads the pre-switch team's stale helpdesk flag |
| REV2-18 | 🟡 | Android 426 gate latches globally: one foreign instance's 426 locks the user out of every account with no in-app escape |
| REV2-19 | 🟡 | Android/iOS markdown serializers strip formatting inside links (and drop the URL around inline code) on every save |
| REV2-20 | 🟡 | resolveSession swallows DB errors as "no session": web clients identity-swap onto the empty sentinel shape; a single resulting 401 force-logs-out desktop clients |
| REV2-21 | 🟡 | iOS Share Extension gates on the ACTIVE account's token, so signing out of one server disables sharing for all still-signed-in accounts |
| REV2-22 | 🟡 | iOS Share Extension: failed image upload strands a half-created issue and each Post retry creates a duplicate |
| REV2-23 | 🟡 | Disabling helpdesk orphans open threads: reporter replies and member fan-out never re-check helpdesk_enabled, while ticket creation does |
| REV2-24 | 🟡 | Relay remote starts enforce one-session-per-issue only via LocalSessions — synced live sessions on other devices are never checked (no server backstop) |
| REV2-25 | 🟡 | Bulk-closing >25 issues permanently skips the widget reporters' one-time resolution emails |
| REV2-26 | 🟡 | Comment edits never resolve @mentions — no subscription, no notification, unlike issue-description edits |
| REV2-27 | 🟡 | issues.create (and bare status-only update) can mint status='duplicate' with no canonical link, and create never stamps completedAt for born-terminal issues |
| REV2-28 | 🟡 | teamMembers.remove leaves ex-members as invisible ghost assignees on team issues |
| REV2-29 | 🟡 | Installation suspend webhook irreversibly cascades away team links; unsuspend cannot self-heal and the settings UI shows repos healthy while token mints 412 |
| REV2-30 | 🟡 | One-subscription-per-team enforced only at checkout mint — webhook binding accepts a second paid subscription that then charges invisibly |
| REV2-31 | 🟡 | Widget keeps filing feedback into ARCHIVED boards — boardArchivedAt is fetched and documented as a gate but never read |
| REV2-32 | 🟡 | Issue detail route shows false "not found" during initial Electric sync on every cold deep link |
| REV2-33 | 🟡 | OAuth sign-in handlers discard Better Auth's resolved error — a failed Google/Apple/OIDC start leaves the login page stuck on a disabled "Redirecting..." with no message |
| REV2-34 | 🟡 | Web UI can neither archive, unarchive, nor hide archived boards — MCP-archived boards become unmanageable zombies in nav |
| REV2-68 | ⚪ | Web image HEALTHCHECK hardcodes port 3000, misreporting health whenever the documented PORT=5173 self-host run isn't fronted by compose Caddy |
| REV2-69 | ⚪ | Widget honeypot is server-side only: no hidden `website` field is ever rendered or sent by the widget client |
| REV2-70 | ⚪ | creem_subscriptions.updated_at frozen at insert — no trigger, no Drizzle $onUpdate, and neither app writers nor the Creem plugin's webhook persistence stamp it |
| REV2-71 | ⚪ | Invite accept runs the seat-cap gate before the already-member no-op, so existing members in full teams get a plan-limit error |
| REV2-72 | ⚪ | bulkUpdate derives status transitions from an unlocked pre-transaction snapshot, bypassing the FOR UPDATE invariant the single-issue path enforces |
| REV2-73 | ⚪ | issueLabels.add/remove log phantom timeline events on no-ops; bulkRemove skips its documented trashed-board eligibility check |
| REV2-74 | ⚪ | Self-hosted PR poller never heals reopened PRs — a closed-then-reopened PR is invisible, and its eventual merge never completes the issues |

### UX & product flow — 18 (`ux`)

| ID | Sev | Finding |
| --- | --- | --- |
| REV2-10 | 🟠 | Support magic-link email failure is recorded but surfaced nowhere — reporter's only credential silently never arrives and no UI (not even admin) can see it |
| REV2-11 | 🟠 | Self-host instructions (marketing docs + README) fail on a fresh clone: env lands where nothing reads it, wrong DB name in apps/web/.env.example, Garage secrets step missing |
| REV2-12 | 🟠 | Auth guards redirect to login with `redirect: undefined`, dropping deep-link destinations (emailed issue links land on default team) |
| REV2-13 | 🟠 | Support unread badge never clears from the Support inbox — support_reply notifications are only marked read via the Inbox tab |
| REV2-50 | 🟡 | Android: issue-detail and Reviews mutations swallow tRPC failures silently (merge PR, delete, duplicate, subscribe, status/priority/assignee, comment delete/send) |
| REV2-51 | 🟡 | Helpdesk email gaps: no support_reply opt-out toggle, linkless support digest items, two sender identities per thread |
| REV2-52 | 🟡 | Unverified-email users silently never get digest emails: prefs UI shows live toggles, but no verify banner or resend path exists |
| REV2-53 | 🟡 | Mobile OAuth failure paths never deep-link back to the app — native auth sheet/tab strands the user with no error shown |
| REV2-54 | 🟡 | Reporter page composer sends on every Enter — mobile reporters cannot write multi-line replies, and a throttled load shows a dead-end error |
| REV2-55 | 🟡 | Account deletion immediately cancels surviving teams' paid subscriptions with no dialog warning and no notice to remaining owners |
| REV2-56 | 🟡 | iOS attachment rows are dead UI: UIApplication.open on the stored relative /api/attachments URL is a silent no-op |
| REV2-57 | 🟡 | Self-host docs omit the steer relay and email transport entirely, though marketing promises "remote steer" and email on every plan and helpdesk on self-host |
| REV2-58 | 🟡 | helpdesk.reply on a resolved thread emails the reporter a reply invitation the revoked magic link then rejects |
| REV2-59 | 🟡 | Board routes hang on eternal "Loading board..." for nonexistent/trashed board slugs instead of a not-found state |
| REV2-60 | 🟡 | Create-issue dialog irreversibly discards a typed draft on Escape or backdrop click |
| REV2-61 | 🟡 | Solo owners have no nav path to Settings → General, making the Delete-team Danger Zone unreachable |
| REV2-86 | ⚪ | Billing portal and checkout buttons fail silently — no toast when the Creem request errors |
| REV2-87 | ⚪ | Global issue search (Cmd+F) has no keyboard model: no arrow-key highlight, Enter does nothing |

### Legacy / stale code — 17 (`legacy-cleanup`)

| ID | Sev | Finding |
| --- | --- | --- |
| REV2-62 | 🟡 | apps/video keeps renderable release-era compositions (incl. the dead exponential_release_pr_open tool) and is missing from CLAUDE.md's workspace contract |
| REV2-63 | 🟡 | Marketing /docs/coding/ contradicts EXP-201/206: false "batch runs start with ultracode on" claim, no mention of the claude/codex/pi agent choice |
| REV2-64 | 🟡 | README.md and .env.example still claim every project is a GitHub repo / GitHub App required to create projects (stale post-EXP-180 vocabulary + repo-optional collapse) |
| REV2-88 | ⚪ | Stale comments referencing dead vocabulary and superseded plans (legacy /w/ URL form, removed public-board model, 'Phase C' auth rework) |
| REV2-89 | ⚪ | Stale pre-EXP-180 "public team" and MCP-OAuth comments in the auth/shape layer, plus the dead session-cookie.ts module |
| REV2-90 | ⚪ | email_deliveries comments document a dead `notification` kind and a defunct per-notification idempotency mechanism |
| REV2-91 | ⚪ | iOS ExpCore JSONB-era leftovers: dead boards columns (github_repo, preview_config) and live JSON-unwrap of plain-GFM description/comment text |
| REV2-92 | ⚪ | pushTokens.register accepts a 'web' platform no client can send — web push is dead schema, not a half-feature |
| REV2-93 | ⚪ | Desktop stale docs/columns: shapes.rs "15 shapes" header, never-synced users email_verified/is_admin, pre-EXP-201 doctor-gate comments in coding_flow.rs |
| REV2-94 | ⚪ | Dead exports in apps/web/src/lib and a desktop LoopbackListener parked on an archived-plan TODO |
| REV2-95 | ⚪ | Windows dropped from the /download/ meta description and llms.txt platform list, contradicting the home description, JSON-LD, and the download page |
| REV2-96 | ⚪ | apps/web/.env.example claims to document 'every variable the web app actually reads' but omits ~20 read vars (SES/SMTP, SELF_HOSTED, CREEM_*, …) |
| REV2-97 | ⚪ | electric-protocol README/fixtures need a post-EXP-180 refresh: dead vocabulary, wrong client list + paths; desktop tests carry a remap shim waiting on it |
| REV2-98 | ⚪ | Stale pre-EXP-180/EXP-194/EXP-201 comments and CLAUDE.md drift: dead anonymous/public-board rationales, workspace vocabulary, phantom is_public and push_subscriptions, wrong coding_session_status values |
| REV2-99 | ⚪ | Stale pre-EXP-180 "public team/board" comments on authz paths describe a weaker access model than the code enforces |
| REV2-100 | ⚪ | Stale pre-EXP-180 moderator/public-team comments in repositories.ts + mcp/tools.ts; CLAUDE.md drift on coding_session_status and digest cadence |
| REV2-101 | ⚪ | Stale public-board-era comments, dead anonymous-viewer branches, and the never-set restrictModeration prop chain survive EXP-180's removal of anonymous access |

---

*Generated by the EXP-177 multi-agent review workflow. Each `REV2-N` issue on the board carries the full evidence, impact analysis, and suggested fix. Every listed finding survived an adversarial refutation pass; issues footed `plausible` warrant a confirming read before a fix, `confirmed` were traced end-to-end.*
