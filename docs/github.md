# GitHub integration — auth, PRs, and merge detection

How Exponential connects to GitHub: the **GitHub App** install model, who mints
which token, how the agent opens PRs, and how a merged PR gets reflected back on
the issue.

## Model: a GitHub App with bot identity

Each instance registers one **GitHub App** (`GITHUB_APP_ID` / `GITHUB_APP_SLUG` /
`GITHUB_APP_PRIVATE_KEY`). Users **install** the App on their repos from the web
app (`/account/integrations`) — there is no per-user OAuth token and nothing to
paste. PRs are authored by the App's bot identity (`…[bot]`).

The token model is **storage-free**: we never persist a GitHub access token. The
server signs a short-lived **App JWT** (RS256, `iss = app_id`) with the App
private key, then exchanges it for a per-repo **installation token** just before
it's needed. Installation tokens are short-lived (~1h) and repo-scoped.

Core lives in `apps/web/src/lib/integrations/github-app.ts`:

- `githubAppConfigured()` — App env present.
- `githubAppInstallUrl()` — `https://github.com/apps/{slug}/installations/new`.
- `appJwt()` — RS256 JWT via Node `crypto.createPrivateKey` (GitHub keys are
  PKCS#1 `BEGIN RSA PRIVATE KEY`, which `crypto` reads directly).
- `resolveRepoInstallationToken(repo)` — `GET /repos/{owner}/{repo}/installation`
  (App JWT) → `installation_id` → `POST /app/installations/{id}/access_tokens` →
  token (cached ~55min). `null` if the App isn't installed on the repo.

`github-pr.ts::resolveRepoToken({ repo })` delegates to
`resolveRepoInstallationToken` (signature kept so callers are unchanged).

`GITHUB_APP_PRIVATE_KEY` is **base64-encoded** in the env var (so the multi-line
PEM survives env / compose / `.env`); the code decodes it with
`Buffer.from(b64, 'base64').toString('utf8')`.

Who uses the token:

- **Server** opens the PR (`createPullRequest`) and reads diffs (`fetchPullFiles`,
  surfaced by `issues.prFiles`) using a freshly-minted installation token.
- **Desktop agent** fetches a per-repo installation token just-in-time from
  `companion.repoToken` (an agent-gated tRPC mutation that verifies the repo
  belongs to the agent's workspace, then returns `resolveRepoInstallationToken`)
  and uses it only for the local git transport — clone / worktree / commit /
  **push** (`crates/agent-core`: `trpc::repo_token`, called from
  `run_pipeline.rs` before push and from `pr_poll.rs` per issue). The desktop
  hosts no longer feed a `githubToken` into agent-core config.

### Install bookkeeping

The App's **Setup URL** (`/api/integrations/github/setup`) catches the
post-install redirect: it reads the `installation_id`, resolves the signed-in
user, and upserts a `github_installations` row (`installation_id`,
`account_login`, `account_type`, `user_id`) so the integrations page can show
"Installed · {account}". The row is bookkeeping/UI only — token resolution goes
through the App JWT + repo lookup, not this table.

`integrations.github.status` returns `{ configured, installed, installUrl,
accounts }`; the integrations page renders an "Install GitHub App" link when not
installed, "Installed · {accounts}" + a "Manage/add repos" link when it is.

> **All token traffic is outbound.** Minting a JWT and exchanging it for an
> installation token are outbound server calls. Only the **webhook** is inbound,
> and it's optional — self-hosted LAN instances work without any public ingress
> (see `GITHUB_POLLING` below).

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
| **Desktop `pr_poll`** | user's machine | agent-core polls open PRs → `reportPr` | Always-on while the app is open. No cloud load. Outbound. Mints a per-repo token via `companion.repoToken`. |
| **Webhook** | **cloud** | `POST /api/webhooks/github` → `applyPrMergeState` | Event-driven, O(merges). HMAC-verified with `GITHUB_WEBHOOK_SECRET`. The App's single app-level webhook auto-delivers for every installation. Matches the issue by exact `pr_url`. |
| **Cron** | **`GITHUB_POLLING=true`** | `bootstrapSelfHosted()` setInterval | Outbound poll of open-PR issues — for instances GitHub cannot reach by webhook (self-hosted behind NAT). Decoupled from `SELF_HOSTED`. |

The webhook route: `apps/web/src/routes/api/webhooks/github.ts`. The cron:
`apps/web/src/lib/bootstrap-self-hosted.ts` (wired in `server-bun.ts`, gated on
`GITHUB_POLLING === 'true'`).

## Why a GitHub App (not an OAuth App)

Exponential is multi-tenant: **users bring their own repos** into workspace
projects — we don't own their repos or orgs. An OAuth App can't auto-deliver
webhooks across arbitrary users' repos, and an OAuth user token won't reliably
cover org repos that restrict third-party OAuth access. The GitHub App solves
both:

- ✅ One **app-level webhook** auto-delivers for every installation — no per-repo
  or per-org webhook wiring.
- ✅ Works across personal **and** org repos (the org owner approves the install).
- ✅ Short-lived, repo-scoped **installation tokens** instead of a broad,
  long-lived user token — nothing sensitive stored.
- ✅ Bot identity — PRs are authored by `…[bot]`, not impersonating the user.

## Environment

```
GITHUB_APP_ID            # numeric App ID (Settings → Developer settings → GitHub Apps)
GITHUB_APP_SLUG          # App URL slug — builds https://github.com/apps/{slug}/installations/new
GITHUB_APP_PRIVATE_KEY   # App PEM private key, base64-encoded: base64 -w0 app.private-key.pem
GITHUB_WEBHOOK_SECRET    # App webhook HMAC secret; webhook → ${BETTER_AUTH_URL}/api/webhooks/github
GITHUB_POLLING           # 'true' to run the outbound merge cron (instances unreachable by webhook)
```

GitHub App configuration:

- **Setup URL:** `${BETTER_AUTH_URL}/api/integrations/github/setup` (catches the
  post-install redirect; "Request user authorization (OAuth) during installation"
  is **off** — bot model).
- **Webhook URL:** `${BETTER_AUTH_URL}/api/webhooks/github`, secret =
  `GITHUB_WEBHOOK_SECRET`; subscribe to the **Pull request** event.
- **Permissions:** Contents (read/write), Pull requests (read/write), Metadata
  (read).
- "Expire user authorization tokens" / "Enable Device Flow" are irrelevant in the
  bot model (no user-to-server tokens).

By deployment:

- **Cloud (prod + staging):** set `GITHUB_APP_*` + `GITHUB_WEBHOOK_SECRET`. Merge
  detection runs off the app-level webhook; leave `GITHUB_POLLING` unset.
- **Self-hosted, publicly reachable:** same as cloud (the webhook works).
- **Self-hosted behind NAT:** set `GITHUB_APP_*`, skip the webhook, and set
  `GITHUB_POLLING=true` for the outbound merge cron.

Each instance (prod / staging / self-host / dev) registers its **own** GitHub App
with its own Setup + Webhook URLs and its own App ID / private key.
