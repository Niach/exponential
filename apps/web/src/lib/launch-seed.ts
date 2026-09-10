// EXP-825: the composer PRESELECTION — what every play button hands the
// Agent page instead of opening a dialog of its own. One shape on all four
// clients (desktop `ChatSeed`, iOS `AgentComposerSeed`, Android `agent?…`
// route args); on the web it rides `/t/$teamSlug/agent`'s search params and
// is consumed once (`routes/t/$teamSlug/agent.tsx`).

export interface LaunchSeed {
  issueIds: string[]
  /** An action id — wins over `issueIds` when both arrive. */
  actionId?: string
  deviceId?: string
  /** Any issue linked to the PR a `pr` input should open pre-picked. */
  prIssueId?: string
  /** Text inserted into an EMPTY draft (a suggestion's description). */
  text?: string
  /** Curated icon name seeding the Create action builtin's `icon` input. */
  icon?: string
}

/** The route's validated search params (all optional strings). */
export interface AgentSearch {
  issues?: string
  action?: string
  pr?: string
  device?: string
  text?: string
  icon?: string
}

const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/** The search params as a seed, or null when none of them is set. Bad
 * issue ids are dropped rather than refused — a link is a shortcut, not a
 * guarantee. */
export function seedFromSearch(search: AgentSearch): LaunchSeed | null {
  const issueIds = (search.issues ?? ``)
    .split(`,`)
    .map((id) => id.trim())
    .filter((id) => UUID_RE.test(id))
  const seed: LaunchSeed = {
    issueIds,
    actionId: search.action || undefined,
    deviceId: search.device || undefined,
    prIssueId: search.pr && UUID_RE.test(search.pr) ? search.pr : undefined,
    text: search.text || undefined,
    icon: search.icon || undefined,
  }
  const empty =
    seed.issueIds.length === 0 &&
    !seed.actionId &&
    !seed.deviceId &&
    !seed.prIssueId &&
    !seed.text &&
    !seed.icon
  return empty ? null : seed
}

/** The inverse: the search params a play button navigates with. Absent
 * fields are omitted so the URL carries only what was picked. */
export function searchFromSeed(seed: Partial<LaunchSeed>): AgentSearch {
  const out: AgentSearch = {}
  if (seed.issueIds && seed.issueIds.length > 0) out.issues = seed.issueIds.join(`,`)
  if (seed.actionId) out.action = seed.actionId
  if (seed.prIssueId) out.pr = seed.prIssueId
  if (seed.deviceId) out.device = seed.deviceId
  if (seed.text) out.text = seed.text
  if (seed.icon) out.icon = seed.icon
  return out
}
