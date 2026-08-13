// In-process PR actor claims (EXP-494). Every in-app PR open/merge records
// "user X just initiated this action" here immediately before the GitHub API
// call, so the webhook echo — which reliably beats the in-app DB write and
// used to fan out ANONYMOUSLY whenever no coding_sessions row survived to
// attribute to (sessionless runs, the 2h staleness sweep after an unclean CLI
// daemon exit, issue-less action runs) — can fire ATTRIBUTED instead: the
// actor is excluded from their own notification and the racing pair's titles
// match byte-for-byte, so deliver()'s dedupe window collapses it.
//
// Single-process by design (same precedent as the electric-proxy semaphore
// and the widget rate-limit buckets): a claim miss (server restart between
// claim and webhook, multi-replica delivery, genuinely out-of-band PR)
// degrades to the pre-existing sessionOwnerFallback behavior — worse
// attribution, never a crash or a lost notification.

interface PrActorClaim {
  userId: string
  // True when the action came in over an agent's MCP credential — gates the
  // host→requester attribution swap in fireAndForgetPrNotify (EXP-432).
  viaAgent: boolean
  expiresAt: number
}

// Long enough to cover any realistic GitHub-call→webhook gap, short enough
// that a crashed claimant can't misattribute a much later out-of-band event
// on the same branch/PR (notification-only blast radius either way).
const CLAIM_TTL_MS = 10 * 60 * 1000
// Runaway backstop; normal population is a handful of in-flight PR actions.
const MAX_CLAIMS = 1000

const claims = new Map<string, PrActorClaim>()

// GitHub repo full names are case-insensitive; branch names are not.
function openKey(repoFullName: string, headBranch: string): string {
  return `open:${repoFullName.toLowerCase()}#${headBranch}`
}

function mergeKey(repoFullName: string, prNumber: number): string {
  return `merge:${repoFullName.toLowerCase()}#${prNumber}`
}

function put(key: string, actor: { userId: string; viaAgent: boolean }): void {
  const now = Date.now()
  for (const [existingKey, claim] of claims) {
    if (claim.expiresAt <= now) claims.delete(existingKey)
  }
  // Refresh insertion order so cap eviction drops the oldest live claim.
  claims.delete(key)
  while (claims.size >= MAX_CLAIMS) {
    const oldest = claims.keys().next().value
    if (oldest === undefined) break
    claims.delete(oldest)
  }
  claims.set(key, { ...actor, expiresAt: now + CLAIM_TTL_MS })
}

// Take = consume. The webhook resolves a claim exactly once per delivery
// (before its per-issue loop, so one claim covers every issue of a batch PR);
// consuming keeps a stale claim from misattributing a later unrelated event,
// and redeliveries can never double-notify anyway (the applyPr* idempotent
// guards).
function take(key: string): { userId: string; viaAgent: boolean } | null {
  const claim = claims.get(key)
  if (!claim) return null
  claims.delete(key)
  if (claim.expiresAt <= Date.now()) return null
  return { userId: claim.userId, viaAgent: claim.viaAgent }
}

export function claimPrOpen(
  repoFullName: string,
  headBranch: string,
  actor: { userId: string; viaAgent: boolean }
): void {
  put(openKey(repoFullName, headBranch), actor)
}

export function claimPrMerge(
  repoFullName: string,
  prNumber: number,
  actor: { userId: string; viaAgent: boolean }
): void {
  put(mergeKey(repoFullName, prNumber), actor)
}

export function takePrOpenClaim(
  repoFullName: string,
  headBranch: string
): { userId: string; viaAgent: boolean } | null {
  return take(openKey(repoFullName, headBranch))
}

export function takePrMergeClaim(
  repoFullName: string,
  prNumber: number
): { userId: string; viaAgent: boolean } | null {
  return take(mergeKey(repoFullName, prNumber))
}

// Failure-path cleanup: a claim written for a GitHub call that then threw
// must not linger and misattribute a later out-of-band event.
export function releasePrOpenClaim(
  repoFullName: string,
  headBranch: string
): void {
  claims.delete(openKey(repoFullName, headBranch))
}

export function releasePrMergeClaim(
  repoFullName: string,
  prNumber: number
): void {
  claims.delete(mergeKey(repoFullName, prNumber))
}

// Test hook.
export function _clearPrActorClaims(): void {
  claims.clear()
}
