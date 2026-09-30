# Exponential in the ChatGPT / Codex plugin directory (EXP-1153)

What OpenAI's plugin platform is in September 2026, what of it makes sense
for Exponential, what this repo ships for it, and the step list to get
listed. Sources: developers.openai.com/plugins (full export at
`/plugins/llms-full.txt`), github.com/openai/mcp-extensions `docs/spec.md`.

## 1. The platform, in one page

An **OpenAI plugin** is one package, published once to the universal
directory shared by ChatGPT and Codex. It bundles up to three things:

| Part | What it is | Exponential today |
| --- | --- | --- |
| Remote MCP server | A public streamable-HTTP endpoint with OAuth 2.1 (MCP authorization spec). ChatGPT scans `tools/list` and reviews every tool's description and annotations. | `https://app.exponential.at/api/mcp`, 92 tools, better-auth OAuth with DCR + PKCE + consent scope picker. Already used by Claude, Cursor and ChatGPT developer mode. |
| Skills | `skills/<name>/SKILL.md` workflows the model follows (provider-neutral, scanned for policy). One may be the **onboarding skill** ChatGPT runs after install. | `apps/marketing/public/SKILL.md` (a discovery file, not packaged). |
| UI (MCP Apps) | Optional iframe components returned by tools, rendered in the conversation. | None. |

**Plugin extensions** (the issue's link) are the ChatGPT-specific layer ON
TOP of MCP Apps: they need a UI resource and are declared as tool `_meta`
(`openai/ui`, `openai/extensions`). The nine of them, and what each would
mean for us:

| Extension | Needs | Verdict for Exponential |
| --- | --- | --- |
| Sidebar app (global entrypoint) | An MCP App rendered fullscreen; must accept `{}` args; monochrome SVG icon. | Later, and NOT by iframing app.exponential.at: the app forbids framing (`frame-ancestors 'none'`) and cookie auth inside a cross-site iframe is dead on Safari/Chrome. A sidebar app would be a purpose-built `@exp/ui` island fed by tool calls over the MCP Apps bridge. |
| Conversation panel (thread entrypoint) | Same, as a per-thread tab. | The first UI worth building: a board view (open issues grouped by status) beside the chat, built from the same islands the styleguide already renders (`@exp/ui/island`). |
| Plugin settings (structured settings) | Two tools (`settings.read`/`settings.update`), server persists. | Low value: the consent grant already scopes teams/boards; a "default board" setting is the only candidate. Skip. |
| File viewers/editors | Desktop only, a file extension we own. | No file type of ours. Skip. |
| Display modes | Only with UI. | With the panel, later. |
| Deep links | Only with a global app. | Later. |
| Model-App context | Only with UI. | Later. |
| Composer @-mentions | Desktop app only. A tool flagged `openai/extensions: { "mentions/search": {} }` taking `{query}` and returning `resource_link`s; needs `resources/read` for the linked issue. | The cheapest high-value extension: `@Exponential EXP-42` typeahead over `lib/issue-search-sql.ts` (the engine already behind `issues.search`/`issues_list.search`), issues exposed as MCP resources. No UI needed. Phase 2. |
| Rich forms (form elicitation) | Elicitation with images/`oneOf`; registered servers need MRTR. | "Which board?" with board icons. Nice, not needed. |

Web availability of extensions is "coming soon" for Free/Go users; the
desktop app has all of them. None of them is a listing prerequisite.

## 2. Recommendation

**Phase 1 (this PR + the portal steps below): list Exponential as an MCP +
skills plugin, no UI.** The server, OAuth and consent screen already meet
the spec; what was missing is the review hygiene (annotations), the
package, domain verification and OIDC discovery. Listing puts Exponential
one click from every ChatGPT and Codex user, including Codex users who run
our coding agents and can then reach the tracker from Codex itself.

**Phase 2: composer mentions + issue resources** (desktop). One tool plus
`resources/read` for `exponential://issues/<IDENT>`; the search engine and
the issue serializer exist.

**Phase 3: a thread panel board view** as an MCP App built from `@exp/ui`
islands, fed by `exponential_issues_list` over the bridge. Only then a
global sidebar app, and only if the panel earns it.

Do not: iframe the web app; build a settings extension; chase file viewers.

## 3. What this PR ships

- **Tool annotations, all 92 tools, one table** (`apps/web/src/lib/mcp/annotations.ts`,
  stamped at registration): `readOnlyHint`/`destructiveHint`/`openWorldHint`
  as explicit booleans, following the directory's strict reading
  (overwrite = destructive; a push to GitHub, an email to a non-member or a
  report to a third party = open world). Mismatched or absent hints are
  the most-cited rejection reason, and the portal's daily rescans hold any
  tool whose hints drift. `api-conventions.test.ts` gates the table against
  the registered set.
- **Domain verification**: `GET /.well-known/openai-apps-challenge` serves
  the bare token from `OPENAI_APPS_CHALLENGE` (root `.env.example`).
- **OIDC discovery at the root**: `GET /.well-known/openid-configuration`
  publishes the same document as `oauth-authorization-server`; ChatGPT
  Enterprise's workspace domain restriction reads `email` +
  `email_verified` from the userinfo endpoint it discovers there.
- **MCP App views** (`packages/mcp-views/`): the board and issue views,
  React on `@exp/ui`, built into one HTML document each and served by
  `lib/mcp/views.ts` as `ui://exponential/{board,issue}`; linked from
  `exponential_board_view` (also ChatGPT's thread-panel entrypoint) and
  `exponential_issue_view`. The views load nothing external (no CSP
  origins; avatars are initials), so the "with UI" review needs only the
  screenshots, which the package ships.
- **The package**: `packages/chatgpt-plugin/` in the portable Agent
  Plugins format: `plugin.json` (listing copy, review test cases,
  publication metadata), `mcp.json` (the one connected server), two skills
  (`exponential` = the workflows, `get-started` = the onboarding skill),
  square icons. `bun run --filter @exp/chatgpt-plugin pack` writes
  `dist/exponential-<version>.zip`; `plugin.test.ts` enforces the
  submission limits (lengths, HTTPS URLs, square icons, real tool names,
  five positive + three negative cases, no pricing or comparative copy,
  bounded screenshots of the views, provider-neutral skills).

## 4. Getting listed: the runbook

Everything below happens outside the repo, in
[platform.openai.com/plugins](https://platform.openai.com/plugins). Order
matters.

1. **Identity.** Complete **business verification** in the OpenAI Platform
   organization settings for the name to publish under ("Exponential").
   Publishing under an unverified name is rejected. The submitter needs
   `api.apps.write` (org owners have it). The project must have **global**
   data residency; an EU-residency project cannot submit MCP plugins.
2. **Set the challenge token.** Upload the ZIP once to reach the MCP
   connect step; the portal shows a token. Set `OPENAI_APPS_CHALLENGE` on
   the cloud web service (Coolify) and redeploy; confirm
   `curl https://app.exponential.at/.well-known/openai-apps-challenge`
   returns exactly the token. Then complete domain verification in the
   portal.
3. **Reviewer account.** Create a dedicated Exponential account on the
   cloud with **email + password** (no OTP, no passkey, no MFA, no magic
   link; the review team rejects anything that needs a second factor).
   Invite it into a demo team with one board whose identifier prefix is
   `DEMO`, seeded so the review cases hold: eight or more open issues,
   DEMO-3 with a description and at least two comments, DEMO-7 with no
   linked pull request. Keep the account and data alive: every later
   review re-uses them. Sign-in instructions for the portal: "sign in with
   the password, then on the consent screen select the Demo team". Enter
   the credentials under **Review details**, never in the ZIP.
4. **Connect and scan.** Under **MCPs** connect
   `https://app.exponential.at/api/mcp`, choose OAuth (DCR: our
   authorization server has no CIMD support, so leave CIMD unselected),
   authenticate as the reviewer account, run **Scan Tools**. The scan
   imports names, descriptions, schemas, annotations and the server
   `instructions`. Every finding must be cleared before submitting; fix on
   the server, deploy, **Rescan**.
5. **Review information.** The five positive and three negative test cases
   ride in `plugin.json` and import with the ZIP. Record the **video
   walkthrough** (all eight cases against the reviewer account) and enter
   its URL in the portal; it is deliberately not in the manifest. Confirm
   the imported countries and release notes.
6. **Submit**, attest to the policies, wait for the email. Public
   experience (screenshot in the issue thread): a few days when the tool
   specifications are precise. No expedite requests; appeals answer the
   rejection email.
7. **Publish** from the portal once approved (approval alone lists
   nothing). Search the directory by the exact name to confirm; front-page
   placement is OpenAI's pick, not ours.
8. **Afterwards.** Update the marketing MCP docs' ChatGPT section
   (`apps/marketing/src/McpDocsPage.tsx`) from "developer mode + paste the
   URL" to the directory link. Coordinate any announcement with
   press@openai.com first. Bump `version` in `plugin.json` and re-upload
   for any change to skills, metadata or `mcp.json`; server-side tool
   changes are picked up by the daily rescan (a held tool keeps its last
   approved definition, a NEW tool stays hidden until approved).

## 5. Review risks specific to us, and what to do

- **Response minimization.** Tools return UUIDs and timestamps. Ids are
  needed for follow-up calls (kept); the skills tell the model not to
  echo them. If the review flags timestamps, trim the wire columns in
  `lib/issue-columns.ts` for the MCP path rather than argue.
- **Helpdesk gating** (`assertCanUseHelpdesk`) refuses on the free plan:
  the refusal must explain, never link to checkout. The current
  `PRECONDITION_FAILED` message does neither, which is fine; the plugin
  copy says plan limits are never surfaced as an upgrade path.
- **Privacy policy** must list categories of personal data, purposes,
  recipients, retention and controls. `exponential.at/privacy/` has all
  five sections; check that OAuth-linked clients ("MCP clients acting as
  you") are named as a recipient category before submitting.
- **Server `instructions`** are part of the scanned metadata. For a
  ChatGPT caller (no session header) they are the three general
  paragraphs only; the coding-run close-out paragraphs never register for
  it (`lib/mcp/gates.ts`).
- **Open-world writes** (`pr_open`, `pr_merge`, `sessions_start`,
  `invites_create`, `helpdesk_reply`, `report_bug`) will make ChatGPT
  confirm before calling. That is the intended posture; do not soften the
  hints to avoid the prompt.
- **Countries.** `plugin.json` lists the EU/EFTA core plus GB, US, CA,
  AU, NZ. Widen or narrow in the portal before submitting.

## 6. Notes for phase 2 and 3

`packages/mcp-views/` IS the phase-3 UI: React views built from `@exp/ui`,
served as `ui://exponential/{board,issue}` by `lib/mcp/views.ts` and linked
from `exponential_board_view` / `exponential_issue_view`; `harness/` is a fake
host for local development.

- Composer mentions: tool `_meta["openai/extensions"]["mentions/search"] = {}`
  and `_meta.ui.visibility = ["app"]`; input `{query}`; output
  `structuredContent.items = [{type: "resource_link", uri, name}]`. Resolve
  `exponential://issues/<IDENT>` via `resources/read` with the same
  serializer `exponential_issues_get` uses. Team scoping = the OAuth grant,
  like every tool.
- Thread panel: an MCP App is an HTML resource (`ui://exponential/board`)
  linked from a tool via `_meta.ui.resourceUri`; the iframe talks JSON-RPC
  over `postMessage` (`ui/initialize`, `tools/call`, `ui/message`). Build
  it as a Vite bundle of `@exp/ui` islands with a CSP naming
  app.exponential.at for images; keep the tool useful without the UI.
  Entry: `_meta["openai/ui"].entrypoints = [{type: "thread"}]`, tool must
  accept `{}`.
- Icons for entrypoints: monochrome SVG, `currentColor`, 20×20 viewport,
  1.33px strokes; the brand mark stays the listing logo.
