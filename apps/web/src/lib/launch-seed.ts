// EXP-825: the composer PRESELECTION — what every play button hands the
// Agent page instead of opening a dialog of its own. One shape on all four
// clients (desktop `ChatSeed`, iOS `AgentComposerSeed`, Android `agent?…`
// route args); on the web it rides `/t/$teamSlug/agent`'s search params and
// is consumed once (`routes/t/$teamSlug/agent.tsx`).

export interface LaunchSeed {
  issueIds: string[]
  deviceId?: string
  /** Text inserted into an EMPTY draft (a suggestion's description). */
  text?: string
}

/** The route's validated search params (all optional strings). */
export interface AgentSearch {
  issues?: string
  device?: string
  text?: string
}

const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/** EXP-1019: the seed carries a SUBJECT — issues (NEW-2: prompts are picked
 *  in the composer's `/` menu, never seeded). That is the
 *  one thing that decides whether a start opens the launcher as a DIALOG
 *  (`use-open-composer.ts`) or travels to the Agent page as a plain chat. */
export function seedHasSubject(seed: Partial<LaunchSeed>): boolean {
  return (seed.issueIds?.length ?? 0) > 0
}

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
    deviceId: search.device || undefined,
    text: search.text || undefined,
  }
  const empty =
    seed.issueIds.length === 0 &&
    !seed.deviceId &&
    !seed.text
  return empty ? null : seed
}

/** The inverse: the search params a play button navigates with. Absent
 * fields are omitted so the URL carries only what was picked. */
export function searchFromSeed(seed: Partial<LaunchSeed>): AgentSearch {
  const out: AgentSearch = {}
  if (seed.issueIds && seed.issueIds.length > 0) out.issues = seed.issueIds.join(`,`)
  if (seed.deviceId) out.device = seed.deviceId
  if (seed.text) out.text = seed.text
  return out
}
