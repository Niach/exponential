// Pure resolvers + serializers for the WebMCP tool layer (EXP-245). Agents
// reference entities the human way — board slug/name, issue identifier,
// assignee email/name, label names — and these resolve those references
// against synced collection snapshots. Misses throw with the valid options in
// the message so the calling agent can self-correct.
import type { Board, Comment, Issue, Label, User } from "@/db/schema"

export function resolveBoard<B extends Pick<Board, `slug` | `name`>>(
  ref: string | undefined,
  currentBoardSlug: string | null,
  boards: B[]
): B {
  const available = boards.map((b) => b.slug).join(`, `) || `none`
  const wanted = ref ?? currentBoardSlug
  if (wanted == null) {
    throw new Error(
      `No board specified and none is open. Pass a board slug. Available boards: ${available}`
    )
  }
  const needle = wanted.toLowerCase()
  const bySlug = boards.find((b) => b.slug.toLowerCase() === needle)
  if (bySlug) return bySlug
  const byName = boards.filter((b) => b.name.toLowerCase() === needle)
  if (byName.length === 1 && byName[0]) return byName[0]
  if (byName.length > 1) {
    throw new Error(
      `Board name "${wanted}" is ambiguous. Use a slug. Available boards: ${available}`
    )
  }
  throw new Error(
    `No board "${wanted}" in this team. Available boards: ${available}`
  )
}

export function resolveIssue<I extends Pick<Issue, `identifier`>>(
  ref: string | undefined,
  currentIssueIdentifier: string | null,
  issues: I[]
): I {
  const wanted = ref ?? currentIssueIdentifier
  if (wanted == null) {
    throw new Error(
      `No issue specified and none is open. Pass an issue identifier like EXP-42.`
    )
  }
  const needle = wanted.toLowerCase()
  const match = issues.find((i) => i.identifier.toLowerCase() === needle)
  if (!match) {
    throw new Error(
      `No issue "${wanted}" in this team. Check the identifier (e.g. via list_issues or search_issues).`
    )
  }
  return match
}

// Resolves an assignee reference (email or display name) to a user id.
// `null` passes through as the explicit unassign sentinel.
export function resolveAssignee(
  ref: string | null,
  users: Pick<User, `id` | `name` | `email`>[]
): string | null {
  return resolveAssigneeRef(ref, users, []).assigneeId
}

/** #EXP-1383: an assignee reference = a member (email or name) or a team
 *  agent (name). Exactly one side is set; `null` unassigns both. */
export function resolveAssigneeRef(
  ref: string | null,
  users: Pick<User, `id` | `name` | `email`>[],
  agents: { id: string; name: string }[]
): { assigneeId: string | null; assigneeAgentId: string | null } {
  if (ref === null) return { assigneeId: null, assigneeAgentId: null }
  const needle = ref.toLowerCase()
  const byEmail = users.find((u) => u.email.toLowerCase() === needle)
  if (byEmail) return { assigneeId: byEmail.id, assigneeAgentId: null }
  const byName = users.filter((u) => u.name.toLowerCase() === needle)
  const agentsByName = agents.filter((a) => a.name.toLowerCase() === needle)
  if (byName.length + agentsByName.length > 1) {
    throw new Error(
      `Assignee name "${ref}" is ambiguous. Use an email address instead.`
    )
  }
  if (byName[0]) return { assigneeId: byName[0].id, assigneeAgentId: null }
  if (agentsByName[0]) return { assigneeId: null, assigneeAgentId: agentsByName[0].id }
  const members = users.map((u) => `${u.name} <${u.email}>`).join(`, `)
  const agentNames = agents.map((a) => a.name).join(`, `)
  throw new Error(
    `No team member matching "${ref}". Members: ${members || `none`}${agentNames ? `. Agents: ${agentNames}` : ``}`
  )
}

export function resolveLabels<L extends Pick<Label, `name`>>(
  names: string[],
  labels: L[]
): L[] {
  return names.map((name) => {
    const needle = name.toLowerCase()
    const match = labels.find((l) => l.name.toLowerCase() === needle)
    if (!match) {
      const available = labels.map((l) => l.name).join(`, `) || `none`
      throw new Error(
        `No label "${name}" in this team. Available labels: ${available}`
      )
    }
    return match
  })
}

function toIso(value: Date | string | null | undefined): string | null {
  if (value == null) return null
  return value instanceof Date ? value.toISOString() : value
}

export function serializeBoard(
  board: Pick<Board, `slug` | `name` | `prefix`>,
  issueCount: number
) {
  return {
    slug: board.slug,
    name: board.name,
    prefix: board.prefix,
    issueCount,
  }
}

/** EXP-1293: the issue's pull request as the synced `pull_requests` row
 *  carries it (the open one, else the newest; null = none). */
export type SerializedIssuePr = { url: string; state: string } | null

export function serializeIssue(
  issue: Pick<
    Issue,
    | `identifier`
    | `title`
    | `status`
    | `priority`
    | `assigneeId`
    | `dueDate`
    | `createdAt`
    | `updatedAt`
    | `source`
  > & { assigneeAgentId?: string | null },
  labelNames: string[],
  userNameById: Map<string, string>,
  pullRequest: SerializedIssuePr = null,
  agentNameById: Map<string, string> = new Map()
) {
  return {
    identifier: issue.identifier,
    title: issue.title,
    // Provenance matters to an agent that also holds the write tools:
    // `widget` rows were filed by an anonymous stranger through the feedback
    // widget, so their title/description is untrusted input, never
    // instructions.
    source: issue.source,
    status: issue.status,
    priority: issue.priority,
    assignee: issue.assigneeId
      ? (userNameById.get(issue.assigneeId) ?? issue.assigneeId)
      : issue.assigneeAgentId
        ? (agentNameById.get(issue.assigneeAgentId) ?? issue.assigneeAgentId)
        : null,
    dueDate: issue.dueDate ?? null,
    labels: labelNames,
    prUrl: pullRequest?.url ?? null,
    prState: pullRequest?.state ?? null,
    createdAt: toIso(issue.createdAt),
    updatedAt: toIso(issue.updatedAt),
  }
}

export function serializeComment(
  comment: Pick<Comment, `id` | `authorId` | `body` | `createdAt`>,
  userNameById: Map<string, string>
) {
  return {
    id: comment.id,
    // SLOP-4: a widget reporter's comment has no author row.
    author:
      comment.authorId === null
        ? `Anonymous visitor`
        : (userNameById.get(comment.authorId) ?? comment.authorId),
    body: comment.body,
    createdAt: toIso(comment.createdAt),
  }
}
