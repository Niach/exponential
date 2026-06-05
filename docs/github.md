# GitHub integration — auth, PRs, and merge detection

How Exponential connects to GitHub: who holds which token, how the agent opens
PRs, and how a merged PR gets reflected back on the issue. Ends with the open
decision on **OAuth App vs GitHub App** for multi-tenant cloud.

## Current model (OAuth App + `linkSocial`)

Each instance registers a **GitHub OAuth App** (`GITHUB_CLIENT_ID` /
`GITHUB_CLIENT_SECRET`). A user connects GitHub **once in the web app** via Better
Auth `linkSocial({ provider: 'github', scopes: ['repo'] })`
(`apps/web/src/routes/_authenticated/account/integrations.tsx`). The token is
stored in the Better Auth `accounts` table and auto-refreshed.

Token resolution lives in `apps/web/src/lib/integrations/github-pr.ts`:

- `resolveOwnerGithubToken(userId)` — the user's connected token.
- `resolveWorkspaceAgentOwnerToken(workspaceId)` — the workspace agent's owner's
  token.
- `resolveRepoToken({ actorUserId?, workspaceId, repo })` — actor → workspace
  agent owner → legacy agent token.

Who uses the token:

- **Server** opens the PR (`createPullRequest`) and reads diffs (`fetchPullFiles`,
  surfaced by `issues.prFiles`) on the user's behalf.
- **Desktop agent** fetches the owner's token from `integrations.github.token` (a
  `.query`, called under the **human session** — not the agent credential) and
  uses it to clone / worktree / commit / **push** (`crates/agent-core`). On Linux
  this is `reconcileAgent` → `fetchGithubToken`; on macOS it's
  `MacAgentService.fetchOwnerGithubToken` → agent-core config `githubToken`.

> **OAuth redirect ≠ inbound.** The OAuth redirect goes to the *browser*; the
> token exchange is an *outbound* server call. Self-hosted LAN instances work
> without any public ingress.

One issue = one PR = one branch/worktree. PR state lives on the issue:
`pr_url` / `pr_number` / `pr_state` / `branch` / `pr_merged_at` (synced via
Electric).

## Merge detection

When a PR merges we flip `issues.pr_state = 'merged'`, stamp `pr_merged_at`, and
emit a `pr_merged` activity event. The single shared writer is
`applyPrMergeState` (`apps/web/src/lib/integrations/pr-sync.ts`) — idempotent on
the `open → merged` transition (so two triggers can't double-fire). It mirrors
`agentPlan.reportPr`'s write path but is callable outside tRPC.

Three triggers, by deployment:

| Trigger | Where | Mechanism | Notes |
|---|---|---|---|
| **Desktop `pr_poll`** | user's machine | agent-core polls open PRs → `reportPr` | Always-on while the app is open. No cloud load. Outbound. |
| **Webhook** | **cloud** | `POST /api/webhooks/github` → `applyPrMergeState` | Event-driven, O(merges). HMAC-verified with `GITHUB_WEBHOOK_SECRET`. Matches the issue by exact `pr_url`. |
| **Cron** | **self-hosted only** | `bootstrapSelfHosted()` setInterval, gated `SELF_HOSTED === 'true'` | Outbound poll of open-PR issues — works without inbound reachability. Never runs on cloud (no per-user polling). |

The webhook route: `apps/web/src/routes/api/webhooks/github.ts`. The cron:
`apps/web/src/lib/bootstrap-self-hosted.ts` (wired in `server-bun.ts`).

## The webhook-delivery problem (multi-tenant)

A webhook only fires if **something is configured to send it**. In Exponential,
**users add their *own* repos** to workspace projects — we don't control their
repos or orgs. That rules out the easy options:

- ❌ **Org webhook** — we don't own the users' orgs.
- ❌ **Manual per-repo webhook** — we can't (and shouldn't) ask each user to wire
  webhook settings by hand.

What's left for cloud:

### Option A — auto-register a repo webhook with the user's token (stopgap)

When the agent opens its first PR in a repo, the server already holds the user's
`repo`-scoped token. `repo` scope can also create a webhook
(`POST /repos/{owner}/{repo}/hooks`) on repos the user **admins**, pointing at the
cloud endpoint with `GITHUB_WEBHOOK_SECRET`. No GitHub App, no manual setup.

- ✅ Works today with the existing OAuth App.
- ⚠️ Only where the connecting user is a repo **admin** — true for personal repos;
  an org repo where they're a member/writer will 403.
- ⚠️ Silently mutates the user's repo settings (less transparent than an install).

### Option B — GitHub App (the canonical multi-tenant answer)

The user **installs** the Exponential GitHub App on their repos. The app has a
single **app-level webhook** that auto-delivers events for *every* installation,
plus short-lived per-repo **installation tokens** instead of a broad long-lived
user token.

- ✅ Reliable across personal **and** org repos.
- ✅ Single webhook config; no per-repo wiring.
- ✅ Better security (scoped, short-lived install tokens — the handoff's planned
  "hardening upgrade").
- ⚠️ Bigger build: a new install flow + installation-token plumbing, replacing the
  per-user OAuth token for git ops (`resolveRepoToken` and the desktop
  token-fetch would switch to installation tokens).

## Recommendation

Genuinely multi-tenant (users bring their own repos) → **a GitHub App is the
proper solution.** An OAuth App cannot auto-deliver webhooks across arbitrary
users' repos, and only the App reliably covers org repos.

In the meantime we are **not blind**: the desktop **`pr_poll`** already detects
merges while the user's app is open. The webhook/App only adds coverage for the
"app closed" window. So:

- **Now:** ship merge detection on `pr_poll` (already there). Optionally add
  Option A as a stopgap *if* most users connect personal repos.
- **Next:** build the **GitHub App** (Option B) as the real multi-tenant +
  security step, and migrate `resolveRepoToken` / the desktop token-fetch onto
  installation tokens.

## Environment

```
GITHUB_CLIENT_ID            # OAuth App (Settings → Developers)
GITHUB_CLIENT_SECRET        # callback: ${BETTER_AUTH_URL}/api/auth/callback/github
GITHUB_WEBHOOK_SECRET       # cloud merge webhook HMAC; webhook → ${BETTER_AUTH_URL}/api/webhooks/github
```

- **Cloud (prod + staging):** set all three. Merge detection runs off the webhook
  (no cron). Until a GitHub App or Option A exists, webhook deliveries require a
  webhook source on the user's repo — so cloud effectively relies on `pr_poll`
  today for users' repos.
- **Self-hosted:** `GITHUB_WEBHOOK_SECRET` is optional; set `SELF_HOSTED=true` to
  run the outbound merge cron instead (no inbound reachability needed).
- `EXPONENTIAL_GITHUB_OAUTH_CLIENT_ID` (the old device-flow client id) is legacy
  and unused — the desktop device flow has been removed.
