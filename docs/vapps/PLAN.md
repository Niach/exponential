# vApps: the architecture, brainstormed again

State of EXP-1174 after three rounds (2026-10-03 to 2026-10-04). Round 1 added mascots, round 2 made them prompted, round 3 parked them: the production server is a CPU-only Hetzner box, so nothing generative can live server-side, and a parts-based mascot is not worth its ×4 renderer. The brand stays **vapps**. What stays from the rounds: the Discord-shaped consumer shell, the thread as the one conversation unit, visual runs, the Caretaker, claims, the host on your own computer. The decision record is VAPP-1 (D1 to D19); this file is the brainstorm that feeds D20.

Drafts: `drafts/shell-web.webp` and `drafts/home3.webp` (the rail shell), older rounds kept for reference.

## 1. What we have (October 2026)

- **Backend**: Postgres + Electric (21 shapes, member-only, trash-scoped), tRPC, MCP (`/api/mcp`, OAuth, tool-search budget), Better Auth. Teams, boards, issues as threads (EXP-1162), comments, relations, statuses, labels, drafts, attachments, notifications (push + digest), the widget as issues (SLOP-4), billing (cloud only), admin.
- **Devices**: registered machines with shared-team scoping, agent accounts and usage, worktrees, the one run model (SLOP-3: chat, issue, batch, action), actions with triggers (SLOP-2), results screenshots, the steer relay (presence, tickets, activity), the push relay.
- **Hosts**: the gpui desktop IDE and the headless CLI daemon, one ACP engine over the user's own Claude/Codex, the launcher, the playbook, PR flow through the GitHub App.
- **Clients ×4** with parity gates: styleguide, shots, design tokens, icons, the contract, fixture-locked specs.
- **Spikes done**: WebRTC browser ↔ str0m + mobile core (VAPP-3, GO), StyleX-subset layout on taffy ×3 (VAPP-4, GO with a protocol change).
- **Decided**: D1 to D19 on VAPP-1; the two brands (D15); the sequence unslop → SLOP-18 → renderers → shells.

## 2. The nouns, Discord-shaped

| In the vapps app | Underneath | In Exponential |
| --- | --- | --- |
| **server** (the rail) | a team | the team |
| **machines** | the team's devices | Devices |
| **apps** | `vapps` rows of the team, each installed on one machine (or on none, see 4) | the vApps nav slot (D8/D11) |
| **threads** | the team's boards' General threads (VAPP-71) + one thread per app (the app's board) | boards and issues |
| **people** | members | Members |
| **Browse** (rail bottom) | blueprints + packages | the store (VAPP-27) |

Your own computer is your own server: the host app registers the device and the team comes with it. A vapp belongs to a server and runs on one of its machines; sharing one vapp into another server is a grant (VAPP-65). The consumer shell never says team, board, issue, run, device; it says server, thread, app, machine, people.

## 3. Layers

```
L5  shells      Exponential (pro: slots nav/home/detail/inline/question/settings, D11)
                vapps (consumer: rail = servers; a server = apps + threads + machines + people)
L4  renderers   web (@exp/ui) · gpui · SwiftUI · Compose, on crates/vapp-client (reducer + styles + taffy)
L3  hosts       crates/vapp on desktop + CLI: compose lifecycle for HOSTED apps (declarative apps need no host)
L2  SDK         ONE protocol anyone can implement: manifest (tools, events, surfaces, permissions) ·
                A2UI v1.0 + the exp catalog + Box styles · BINDINGS (local data sources + local tools) ·
                the Bun server plugin for backends · the package format for declarative apps
L1  peer link   WebRTC data channels, identity keys + QR, our TURN, relay signaling, sealed mailbox,
                device MCP gateway (VAPP-36 to VAPP-52)
L0  backend     identity, teams, devices, boards/threads, sessions, actions, MCP, sync, attachments, notifications
```

L0 and L1 are decided and partly built. L2 is where this round changes things: **bindings** and **declarative apps**.

## 4. Two kinds of vapps

**Declarative vapp** = a manifest + A2UI templates + bindings. No code, no process, no host. Its data is Exponential's data (the app's board: issues, comments, attachments, labels, people) plus, when that is not enough, **records** (below). Its logic is the catalog's local behaviours plus Exponential's MCP tools called from actions. It installs instantly on any server, runs on every client including web and phones without a machine, and is safe to share as a **package** because it is data, not code (no prompt injection, no execution).

**Hosted vapp** = a declarative vapp plus a backend (compose on one of the server's machines, D5), reached over the peer link. Everything decided for phase 1 (VAPP-5 to VAPP-22) applies. Shared as a blueprint (D12/D13), built by your own agent on your own machine.

Most consumer apps (recipe box, chore wheel, trip planner, a questionnaire, a radar that files issues) are declarative. Hosted is for real backends (a scraper, an integration with secrets, heavy data).

**Records** (the one backend addition): `vapp_records (id, vapp_id, team_id, kind, data jsonb, created_by, updated_at, deleted_at)`, one team-scoped shape, three MCP tools, quota per plan. A generic row per app so a declarative app can own data that is not an issue. Storage is the only server cost and it is small. Nothing else moves to the server: no GPU, no execution, no app code.

**Building a vapp** still needs an agent on a machine with a subscription (the run that writes the templates), so a server without a machine can install and use apps but not create them; that is the "Where should it run?" card of VAPP-77, now only for creation.

## 5. How rich can A2UI go

A2UI v1.0 is a component tree + a data model + actions, nothing more; the richness is whatever the catalog offers and whatever the client runs locally.

| Can do today (with the exp catalog + Box styles) | With a catalog component we add | Not A2UI's job |
| --- | --- | --- |
| any static layout: flex, grid, absolute, aspect, responsive by surface width | windowed lists (`List`), sheets/dialogs as surfaces, tabs, pull-to-refresh, drag reorder (`SortableList` + a reorder action), charts and graphs (VAPP-50), markdown editing (`MarkdownEditor` wraps TipTap/cmark/comrak/commonmark), diff view (`Diff` wraps the ×4 diff), the transcript (`Transcript` wraps the steer view), maps, video | per-frame logic, games, free drawing (Canvas + a client sandbox, VAPP-30), native gestures beyond what a component exposes, offline logic (the data model is the host's, unless bound locally) |
| text, markdown (read), images, video/audio players, inputs, buttons, pickers, switches, bands, rows, pills, chips, avatars, icons by concept | | |
| local behaviours: hover, pressed, focus, typing, scroll, tab switch | | |
| one round trip per semantic interaction (host-owned state) | | |

**Can Exponential itself run in A2UI?** Yes, with two additions and one rule:

1. **Bindings**: a surface's data model may be bound to a client-side source (`exp:issues?board=…`, `exp:sessions?team=…`, `exp:records?vapp=…`) instead of host messages, and an action may target a local tool (tRPC or MCP as the viewer). Then the client already has the rows (Electric), reads are instant and offline, and a host is optional. This is the piece A2UI does not define and we would.
2. **The catalog wraps every native primitive** we already draw. SLOP-18's inventory IS the catalog list: band, row, pill, chip, glyph, composer box, results tile, question card, toast, plus the heavy ones above (editor, transcript, diff). The natives keep drawing them natively; the template only says where.
3. **The rule**: first-party screens become templates + bindings, never "native screens beside templates". Screen by screen, simplest first (Devices, Actions, Settings, Reviews list), the thread and the transcript last.

What it buys: every screen is written ONCE as a template instead of four times in TypeScript, Swift, Kotlin and Rust; the renderers are the only ×4 code left; the shots and fixtures gate the renderers, not forty screens. It is also the proof that the SDK is complete: if the Devices page renders from the SDK, a third party can replace it (D11) and another harness can render our apps. Costs: the measure round trips over FFI (VAPP-4: 12 ms for 200 nodes on an iPhone simulator, 130 ms on the Android emulator, so lists must be windowed), text metrics drift, accessibility order, debugging a tree instead of code. Verdict: the shell stays native for now; one real screen (Devices) goes first as the dogfood; nothing else is committed until that screen is shipped on four clients.

## 6. The plugin story (the initial plan)

Exponential's shell is slots (D11). A vapp installed in a team shows in the nav slot, its surfaces in the detail slot, its cards in the inline slot of a run, its questions in the question slot. The same `vapps` row, the same templates, the same renderer in both shells. "One SDK others can implement" means three things, in this order: (a) the protocol package (`packages/vapp-protocol`: A2UI pin, exp catalog, Box styles, manifest, events, bindings), (b) our four renderers as the reference, (c) the package format for declarative apps and the Bun plugin for hosted ones. Other harnesses render our apps by implementing (a); other servers plug in as providers over MCP (D10).

## 7. What the CPU-only server means

Nothing generative, nothing executing on Hetzner: no image generation fallback (VAPP-74 becomes host-only), no mascots, no vector search of generated art. The server does identity, sync, records, attachments, notifications, blueprints/packages, the public bridge (a WebSocket relay of A2UI streams for public apps, D18) and the Caretaker's bookkeeping. App icons stay the curated glyph + color that boards use.

## 8. Sequence

1. Unslop (SLOP-1) until Exponential is in a good state.
2. SLOP-18: the primitive inventory, which IS the exp catalog list.
3. VAPP-5 (protocol) extended with bindings and the package format; VAPP-8 (schema) extended with `owner`/`team` rules and records.
4. The web renderer (VAPP-13) + the first dogfood screen (Devices) as a template on web; then the three native renderers (VAPP-14 to VAPP-16) proven on that same screen.
5. The vapps shell: web `/v/*` (VAPP-70, the rail), then iOS + Android (VAPP-75), the host tray + CLI alias (VAPP-76), creation onboarding (VAPP-77).
6. Declarative apps end to end: records, packages, Browse (VAPP-27 narrowed to packages + blueprints).
7. Hosted apps: the peer-link stack (VAPP-36 to VAPP-52), the host (VAPP-12), the SDK server plugin (VAPP-6), the Studio (VAPP-18) in Exponential.
8. Public bridge + claims (VAPP-24, VAPP-29), sharing (VAPP-65), brand (VAPP-78).

## 9. Edit map (run once D20 is confirmed)

| Issue | Change |
| --- | --- |
| VAPP-1 | D20: declarative vs hosted vapps, bindings, records, the dogfood rule, the rail shell; mascots parked |
| VAPP-69 | the nouns table (section 2), the two kinds, sub-issues; mascot sub-issues cancelled |
| VAPP-5 | bindings + package format + the catalog = SLOP-18's inventory |
| VAPP-8 | records table + shape + MCP tools; `vapps.kind` declarative/hosted; `device_id` nullable |
| VAPP-13 to VAPP-16 | each renderer is accepted when the Devices page renders from a template on that client |
| VAPP-17, VAPP-18 | the Exponential side: nav slot list; Studio only for hosted apps |
| VAPP-27, VAPP-25 | Browse = packages (declarative, data) + blueprints (hosted, prompts) |
| VAPP-70, VAPP-75, VAPP-76, VAPP-77 | the rail shell; creation needs a machine, use does not |
| VAPP-74 | host-only, no server fallback |
| VAPP-64 | unchanged, now the mechanism for the dogfood screens too |
| VAPP-79, VAPP-80 | cancelled (mascots parked) |
| VAPP-81 | stays (A2UI drafts + replay) |
| SLOP-18 | "the inventory is the catalog" written into the goal |
| EXP-1118 | still later; bindings + templates make it less necessary |

## Mockup recipe

```sh
cd docs/vapps/mockups && mkdir -p out
node render.mjs shell-web:1280:800:1.5 home3:390:844:2
node montage.mjs
```

`render.mjs` resolves Playwright and sharp from the main checkout's `node_modules`; `parts.js` holds the icon snippets, `base.css` the app's tokens. Older rounds (mascots: `slop.js`, `sheet.html`, `create.html`, `bench.py`) are kept as history only.
