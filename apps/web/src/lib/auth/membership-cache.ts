// REV2-7: short-TTL caches for the two membership id-set resolvers every
// Electric shape long-poll renewal re-runs (getUserTeamIds feeds 12 of the 14
// shape where clauses, getReadableUserIdsInTeams feeds the users shape). A
// synced client renews all 14 shapes per ~60s cycle — and instantly on every
// data change — so without this cache membership load scales with sync
// traffic instead of with actual membership churn.
//
// Invalidation is CLEAR-ALL, not per-user, and deliberately so: adding or
// removing one member also changes every co-member's readable-user set, and
// team/user cascade deletes drop memberships for a set of users nobody
// enumerated. Per-user invalidation is incomplete by construction; clear-all
// is trivially correct, and membership mutations are orders of magnitude
// rarer than renewals (cost: one extra query per active user on their next
// cycle). Every teamMembers writer must call invalidateMembershipCaches()
// AFTER its transaction commits (post-commit, so a concurrent renewal cannot
// repopulate the cache with pre-commit data). Current writers:
//   - trpc/team-members.ts  remove
//   - trpc/team-invites.ts  accept
//   - trpc/teams.ts         create, delete
//   - trpc/admin.ts         deleteTeam, deleteUser
//   - trpc/users.ts         deleteAccount
//   - lib/bootstrap-cloud.ts maybePromoteNewUser
// (updateRole changes no id set and needs no invalidation.)
//
// Revocation bound: in-process, a removed member's next shape request
// re-queries immediately (post-commit clear). The 10s TTL is only the safety
// net for a missed writer / manual SQL / future multi-replica deploy — and it
// sits well inside the existing envelope: a live long-poll keeps serving the
// old where clause until the client's next renewal (~60s) anyway, and web
// cookie sessions already ride a 5-minute cookieCache. A stale set yields the
// same sorted where string as pre-change behavior mid-window; on refresh the
// shape handle rotates and the client 409-re-syncs — the designed recovery
// path for real membership changes.
import { TtlPromiseCache } from "@/lib/ttl-promise-cache"

const TTL_MS = 10_000
const MAX_ENTRIES = 5_000

export const userTeamIdsCache = new TtlPromiseCache<string[]>({
  ttlMs: TTL_MS,
  maxEntries: MAX_ENTRIES,
})

export const readableUserIdsCache = new TtlPromiseCache<string[]>({
  ttlMs: TTL_MS,
  maxEntries: MAX_ENTRIES,
})

export function invalidateMembershipCaches(): void {
  userTeamIdsCache.clear()
  readableUserIdsCache.clear()
}
