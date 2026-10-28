import { APP_ACTIONS, actionGrantable } from "@exp/domain-contract/app-actions"

// FEED-159: an app's `granted_tools` outlives the rule that allowed it. When
// an action becomes owner-only or destructive (fixtures/app-actions.json
// `ownerActions` / `destructiveWords`, shipped with a deploy) or leaves the
// catalog, the names already granted stay stored. Two guards: every READ
// keeps only grantable names (`grantableTools`; `grantAllows` re-checks per
// call), and every boot strips the stored ones (bootstrap-cloud
// `stripUngrantableGrants`), so the synced row and older clients see the
// downgrade. Client-safe: no DB import.

/** The offered actions a grant may name today. */
export const GRANTABLE_ACTIONS: readonly string[] = APP_ACTIONS.filter((name) => actionGrantable(name))

/** `names` without anything no longer grantable (stored order kept). */
export function grantableTools(names: readonly string[] | null | undefined): string[] {
  return (names ?? []).filter((name) => actionGrantable(name))
}
