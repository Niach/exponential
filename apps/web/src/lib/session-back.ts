// S-3 inter-session links (web): an xref tap opens that session and PUSHES
// the session on show onto `back`, the stack of session ids carried in the
// route search (`back=<oldest>,…,<newest>`); the header's `‹` pops it. The
// thread, session and agent routes all read it.

/** The deepest stack kept (the oldest entries fall off). */
export const BACK_STACK_CAP = 10

/** The stack in a route's `back` search value (unknown input = empty). */
export function parseBack(raw: unknown): string[] {
  if (typeof raw !== `string`) return []
  return raw
    .split(`,`)
    .map((id) => id.trim())
    .filter(Boolean)
    .slice(-BACK_STACK_CAP)
}

/** The route search value of a stack (empty = absent). */
export function backParam(stack: readonly string[]): string | undefined {
  return stack.length > 0 ? stack.slice(-BACK_STACK_CAP).join(`,`) : undefined
}

/** `validateSearch`'s field: the normalised value, else nothing. */
export function backSearch(raw: unknown): { back?: string } {
  const back = backParam(parseBack(raw))
  return back ? { back } : {}
}

/** Push the session on show (null = nothing to return to); a repeat of the
 *  top is kept once. */
export function pushBack(stack: readonly string[], here: string | null): string[] {
  if (!here || stack.at(-1) === here) return [...stack].slice(-BACK_STACK_CAP)
  return [...stack, here].slice(-BACK_STACK_CAP)
}

/** Where a session opens on web. */
export type SessionTarget =
  /** Its thread (a child = `?tab=session&session=<id>` in place). */
  | { to: `thread`; threadId: string; session: string | null }
  /** The viewer's own conversation with a team agent = the agent's page. */
  | { to: `agent`; agentId: string }
  /** `/sessions/$id` (a session under an agent's tree, or one not synced
   *  yet: the route resolves it). */
  | { to: `session`; sessionId: string }

export interface TargetSession {
  id: string
  rootId?: string | null
  parentId?: string | null
  agentId?: string | null
  userId?: string | null
}

export function sessionTarget(
  sessionId: string,
  lookup: (id: string) => TargetSession | undefined,
  viewerId: string | null
): SessionTarget {
  const row = lookup(sessionId)
  if (!row) return { to: `session`, sessionId }
  const rootId = row.rootId ?? row.id
  const root = rootId === row.id ? row : lookup(rootId)
  if (!root) return { to: `session`, sessionId }
  if (root.agentId) {
    return row.id === root.id && root.userId === viewerId
      ? { to: `agent`, agentId: root.agentId }
      : { to: `session`, sessionId }
  }
  return { to: `thread`, threadId: root.id, session: row.id === root.id ? null : row.id }
}
