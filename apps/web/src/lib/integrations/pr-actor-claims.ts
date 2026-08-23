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

// ---------------------------------------------------------------------------
// Agent issue activity (EXP-617)
//
// The claims above are PER-PR-EVENT and only exist when the PR flowed through
// our own server. They cannot cover the reported case: an agent working a
// session on issue A files issue B mid-session, implements it, and its PR is
// linked to B — no coding_sessions row names B, so the actor ladder falls all
// the way through and the ANONYMOUS fan-out reaches the very human whose agent
// did the work (they are B's auto-subscribed creator).
//
// This second, coarser record answers a different question: "did a human's
// agent credential WRITE to this issue recently?" It is deliberately
// EXCLUSION-ONLY — it never supplies a title, because "your agent touched this
// issue" is not evidence of who opened the PR, only evidence that notifying
// that person about it would be telling them what they already know.
//
// Three properties that differ from the PR claims on purpose:
//   - keyed on the ISSUE, not on a repo/branch/PR number, so it survives every
//     branch-naming and session-shape mismatch;
//   - a SET of users per issue (two people's agents may both have touched it);
//   - PEEKED, never consumed — one record has to cover the `opened` webhook,
//     the tool's own fan-out, and any later merge.
// Same single-process caveat as above: a miss degrades to today's behavior.

// Short on purpose. This window IS the mitigation for the one wrong-suppression
// risk the mechanism carries (a teammate whose agent touched the issue misses a
// PR notification someone else caused) — do not stretch it to match CLAIM_TTL_MS.
const AGENT_ACTIVITY_TTL_MS = 30 * 60 * 1000

const agentIssueActivity = new Map<string, Map<string, number>>()

export function noteAgentIssueActivity(issueId: string, userId: string): void {
  const now = Date.now()
  for (const [key, actors] of agentIssueActivity) {
    for (const [actorId, expiresAt] of actors) {
      if (expiresAt <= now) actors.delete(actorId)
    }
    if (actors.size === 0) agentIssueActivity.delete(key)
  }
  const existing = agentIssueActivity.get(issueId)
  const actors = existing ?? new Map<string, number>()
  if (!existing) {
    // Refresh insertion order the same way put() does, so the cap below drops
    // the oldest issue rather than an arbitrary one.
    while (agentIssueActivity.size >= MAX_CLAIMS) {
      const oldest = agentIssueActivity.keys().next().value
      if (oldest === undefined) break
      agentIssueActivity.delete(oldest)
    }
    agentIssueActivity.set(issueId, actors)
  }
  actors.set(userId, now + AGENT_ACTIVITY_TTL_MS)
}

// Live actors for an issue. Peek, NOT take (see the header above).
export function peekAgentIssueActors(issueId: string): string[] {
  const actors = agentIssueActivity.get(issueId)
  if (!actors) return []
  const now = Date.now()
  for (const [actorId, expiresAt] of actors) {
    if (expiresAt <= now) actors.delete(actorId)
  }
  if (actors.size === 0) {
    agentIssueActivity.delete(issueId)
    return []
  }
  return [...actors.keys()]
}

// Test hook.
export function _clearPrActorClaims(): void {
  claims.clear()
  agentIssueActivity.clear()
}
