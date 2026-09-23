// EXP-630: the full paged fetch of a Linear workspace into a `LinearSnapshot`
// — everything the plan and the apply need, stored as `import_jobs.payload`
// so the wizard never re-fetches. Comments and relations come from their
// TOP-LEVEL connections (no nested pagination); an issue's history is nested
// (first: 50) with a follow-up per issue only when it has more.
//
// Measured against the Methode 5 workspace on 2026-09-23: a 50-issue page
// with nested history cost 288 complexity, a 100-comment page 610, the
// metadata query 4,300 — all well under the 10,000 per-query cap.
import { z } from "zod"
import { paginate, type Connection, type LinearClient } from "@/lib/import/linear/client"

export const linearStateTypeValues = [
  `triage`,
  `backlog`,
  `unstarted`,
  `started`,
  `completed`,
  `canceled`,
  `duplicate`,
] as const
export type LinearStateType = (typeof linearStateTypeValues)[number]

const idSchema = z.string().min(1)

export const linearSnapshotSchema = z.object({
  version: z.literal(1),
  fetchedAt: z.string(),
  organization: z.object({ id: idSchema, name: z.string(), urlKey: z.string() }),
  viewer: z.object({ id: idSchema, name: z.string(), email: z.string() }),
  teams: z.array(z.object({ id: idSchema, key: z.string(), name: z.string() })),
  states: z.array(
    z.object({
      id: idSchema,
      name: z.string(),
      type: z.string(),
      color: z.string(),
      position: z.number(),
      teamId: idSchema,
    })
  ),
  labels: z.array(
    z.object({
      id: idSchema,
      name: z.string(),
      color: z.string(),
      teamId: idSchema.nullable(),
      parentId: idSchema.nullable(),
    })
  ),
  users: z.array(
    z.object({
      id: idSchema,
      name: z.string(),
      displayName: z.string(),
      email: z.string(),
      active: z.boolean(),
    })
  ),
  projects: z.array(
    z.object({ id: idSchema, name: z.string(), teamIds: z.array(idSchema) })
  ),
  issues: z.array(
    z.object({
      id: idSchema,
      teamId: idSchema,
      identifier: z.string(),
      number: z.number().int(),
      title: z.string(),
      description: z.string().nullable(),
      priority: z.number().int(),
      createdAt: z.string(),
      updatedAt: z.string(),
      completedAt: z.string().nullable(),
      canceledAt: z.string().nullable(),
      archivedAt: z.string().nullable(),
      dueDate: z.string().nullable(),
      estimate: z.number().nullable(),
      stateId: idSchema,
      assigneeId: idSchema.nullable(),
      creatorId: idSchema.nullable(),
      projectId: idSchema.nullable(),
      parentId: idSchema.nullable(),
      labelIds: z.array(idSchema),
      history: z.array(
        z.object({
          id: idSchema,
          createdAt: z.string(),
          actorId: idSchema.nullable(),
          fromStateId: idSchema.nullable(),
          toStateId: idSchema.nullable(),
          fromAssigneeId: idSchema.nullable(),
          toAssigneeId: idSchema.nullable(),
          fromPriority: z.number().int().nullable(),
          toPriority: z.number().int().nullable(),
          addedLabelIds: z.array(idSchema).nullable(),
          removedLabelIds: z.array(idSchema).nullable(),
        })
      ),
    })
  ),
  comments: z.array(
    z.object({
      id: idSchema,
      issueId: idSchema,
      body: z.string(),
      createdAt: z.string(),
      updatedAt: z.string().nullable(),
      editedAt: z.string().nullable(),
      userId: idSchema.nullable(),
      parentId: idSchema.nullable(),
    })
  ),
  relations: z.array(
    z.object({
      id: idSchema,
      type: z.string(),
      issueId: idSchema,
      relatedIssueId: idSchema,
    })
  ),
  // Byte sizes of the upload URLs the descriptions and comments reference
  // (HEAD-probed at discovery; missing = the probe failed).
  assetSizes: z.record(z.string(), z.number().int().nonnegative()),
})
export type LinearSnapshot = z.infer<typeof linearSnapshotSchema>

// --- queries ---------------------------------------------------------------

export const VIEWER_QUERY = `{
  viewer { id name email }
  organization { id name urlKey }
}`

export const METADATA_QUERY = `{
  teams(first: 100) { nodes { id key name } }
  workflowStates(first: 250) { nodes { id name type color position team { id } } }
  users(first: 250) { nodes { id name displayName email active } }
  projects(first: 250) { nodes { id name teams(first: 50) { nodes { id } } } }
}`

export const LABELS_QUERY = `query($first: Int!, $after: String) {
  issueLabels(first: $first, after: $after) {
    pageInfo { hasNextPage endCursor }
    nodes { id name color team { id } parent { id } }
  }
}`

export const HISTORY_FIELDS = `id createdAt actor { id } fromState { id } toState { id } fromAssignee { id } toAssignee { id } fromPriority toPriority addedLabelIds removedLabelIds`

export const ISSUES_QUERY = `query($teamId: ID!, $first: Int!, $after: String) {
  issues(filter: { team: { id: { eq: $teamId } } }, includeArchived: true, first: $first, after: $after, orderBy: createdAt) {
    pageInfo { hasNextPage endCursor }
    nodes {
      id identifier number title description priority
      createdAt updatedAt completedAt canceledAt archivedAt dueDate estimate
      state { id } assignee { id } creator { id } project { id } parent { id }
      labels(first: 50) { nodes { id } }
      history(first: 50) { pageInfo { hasNextPage endCursor } nodes { ${HISTORY_FIELDS} } }
    }
  }
}`

export const ISSUE_HISTORY_QUERY = `query($issueId: String!, $first: Int!, $after: String) {
  issue(id: $issueId) {
    history(first: $first, after: $after) { pageInfo { hasNextPage endCursor } nodes { ${HISTORY_FIELDS} } }
  }
}`

export const COMMENTS_QUERY = `query($teamId: ID!, $first: Int!, $after: String) {
  comments(filter: { issue: { team: { id: { eq: $teamId } } } }, includeArchived: true, first: $first, after: $after) {
    pageInfo { hasNextPage endCursor }
    nodes { id body createdAt updatedAt editedAt user { id } issue { id } parent { id } }
  }
}`

export const RELATIONS_QUERY = `query($first: Int!, $after: String) {
  issueRelations(first: $first, after: $after) {
    pageInfo { hasNextPage endCursor }
    nodes { id type issue { id } relatedIssue { id } }
  }
}`

// --- wire shapes -------------------------------------------------------------

type Ref = { id: string } | null

interface WireHistory {
  id: string
  createdAt: string
  actor: Ref
  fromState: Ref
  toState: Ref
  fromAssignee: Ref
  toAssignee: Ref
  fromPriority: number | null
  toPriority: number | null
  addedLabelIds: string[] | null
  removedLabelIds: string[] | null
}

interface WireIssue {
  id: string
  identifier: string
  number: number
  title: string
  description: string | null
  priority: number
  createdAt: string
  updatedAt: string
  completedAt: string | null
  canceledAt: string | null
  archivedAt: string | null
  dueDate: string | null
  estimate: number | null
  state: Ref
  assignee: Ref
  creator: Ref
  project: Ref
  parent: Ref
  labels: { nodes: { id: string }[] }
  history: Connection<WireHistory>
}

interface WireComment {
  id: string
  body: string
  createdAt: string
  updatedAt: string | null
  editedAt: string | null
  user: Ref
  issue: Ref
  parent: Ref
}

export const LINEAR_UPLOAD_URL_PATTERN = /https:\/\/uploads\.linear\.app\/[^\s)<>"'\]]+/g

export function extractUploadUrls(text: string | null | undefined): string[] {
  if (!text) return []
  const seen = new Set<string>()
  for (const match of text.matchAll(LINEAR_UPLOAD_URL_PATTERN)) {
    seen.add(match[0])
  }
  return [...seen]
}

function mapHistory(node: WireHistory): LinearSnapshot[`issues`][number][`history`][number] {
  return {
    id: node.id,
    createdAt: node.createdAt,
    actorId: node.actor?.id ?? null,
    fromStateId: node.fromState?.id ?? null,
    toStateId: node.toState?.id ?? null,
    fromAssigneeId: node.fromAssignee?.id ?? null,
    toAssigneeId: node.toAssignee?.id ?? null,
    fromPriority: node.fromPriority,
    toPriority: node.toPriority,
    addedLabelIds: node.addedLabelIds,
    removedLabelIds: node.removedLabelIds,
  }
}

export interface SnapshotProgress {
  step: string
  done: number
  total: number
}

export async function fetchLinearSnapshot(
  client: LinearClient,
  options: {
    onProgress?: (progress: SnapshotProgress) => Promise<void>
    probeAssetSize?: (url: string) => Promise<number | null>
    now?: () => Date
  } = {}
): Promise<LinearSnapshot> {
  const progress = async (step: string, done: number, total: number) => {
    await options.onProgress?.({ step, done, total })
  }

  const who = await client.query<{
    viewer: { id: string; name: string; email: string }
    organization: { id: string; name: string; urlKey: string }
  }>(VIEWER_QUERY)

  const meta = await client.query<{
    teams: { nodes: { id: string; key: string; name: string }[] }
    workflowStates: {
      nodes: {
        id: string
        name: string
        type: string
        color: string
        position: number
        team: Ref
      }[]
    }
    users: {
      nodes: {
        id: string
        name: string
        displayName: string
        email: string
        active: boolean
      }[]
    }
    projects: { nodes: { id: string; name: string; teams: { nodes: { id: string }[] } }[] }
  }>(METADATA_QUERY)

  const labels: LinearSnapshot[`labels`] = []
  for await (const page of paginate<{
    id: string
    name: string
    color: string
    team: Ref
    parent: Ref
  }>(async (first, after) => {
    const data = await client.query<{ issueLabels: Connection<never> }>(LABELS_QUERY, {
      first,
      after,
    })
    return data.issueLabels
  })) {
    for (const node of page) {
      labels.push({
        id: node.id,
        name: node.name,
        color: node.color,
        teamId: node.team?.id ?? null,
        parentId: node.parent?.id ?? null,
      })
    }
  }

  const issues: LinearSnapshot[`issues`] = []
  const comments: LinearSnapshot[`comments`] = []
  for (const team of meta.teams.nodes) {
    for await (const page of paginate<WireIssue>(async (first, after) => {
      const data = await client.query<{ issues: Connection<WireIssue> }>(ISSUES_QUERY, {
        teamId: team.id,
        first,
        after,
      })
      return data.issues
    })) {
      for (const node of page) {
        const history = node.history.nodes.map(mapHistory)
        let historyPage = node.history
        while (historyPage.pageInfo.hasNextPage && historyPage.pageInfo.endCursor) {
          const more = await client.query<{ issue: { history: Connection<WireHistory> } }>(
            ISSUE_HISTORY_QUERY,
            { issueId: node.id, first: 100, after: historyPage.pageInfo.endCursor }
          )
          historyPage = more.issue.history
          history.push(...historyPage.nodes.map(mapHistory))
        }
        issues.push({
          id: node.id,
          teamId: team.id,
          identifier: node.identifier,
          number: node.number,
          title: node.title,
          description: node.description,
          priority: node.priority,
          createdAt: node.createdAt,
          updatedAt: node.updatedAt,
          completedAt: node.completedAt,
          canceledAt: node.canceledAt,
          archivedAt: node.archivedAt,
          dueDate: node.dueDate,
          estimate: node.estimate,
          stateId: node.state?.id ?? ``,
          assigneeId: node.assignee?.id ?? null,
          creatorId: node.creator?.id ?? null,
          projectId: node.project?.id ?? null,
          parentId: node.parent?.id ?? null,
          labelIds: node.labels.nodes.map((label) => label.id),
          history,
        })
      }
      await progress(`issues`, issues.length, issues.length)
    }

    for await (const page of paginate<WireComment>(
      async (first, after) => {
        const data = await client.query<{ comments: Connection<WireComment> }>(
          COMMENTS_QUERY,
          { teamId: team.id, first, after }
        )
        return data.comments
      },
      { pageSize: 100 }
    )) {
      for (const node of page) {
        if (!node.issue?.id) continue
        comments.push({
          id: node.id,
          issueId: node.issue.id,
          body: node.body,
          createdAt: node.createdAt,
          updatedAt: node.updatedAt,
          editedAt: node.editedAt,
          userId: node.user?.id ?? null,
          parentId: node.parent?.id ?? null,
        })
      }
      await progress(`comments`, comments.length, comments.length)
    }
  }

  const relations: LinearSnapshot[`relations`] = []
  for await (const page of paginate<{
    id: string
    type: string
    issue: Ref
    relatedIssue: Ref
  }>(
    async (first, after) => {
      const data = await client.query<{ issueRelations: Connection<never> }>(
        RELATIONS_QUERY,
        { first, after }
      )
      return data.issueRelations
    },
    { pageSize: 100 }
  )) {
    for (const node of page) {
      if (!node.issue?.id || !node.relatedIssue?.id) continue
      relations.push({
        id: node.id,
        type: node.type,
        issueId: node.issue.id,
        relatedIssueId: node.relatedIssue.id,
      })
    }
  }

  // Asset sizes: one HEAD per unique upload URL, bounded concurrency. A
  // failed probe just leaves the size unknown (the dry run counts 0 for it).
  const assetSizes: Record<string, number> = {}
  if (options.probeAssetSize) {
    const urls = new Set<string>()
    for (const issue of issues) for (const url of extractUploadUrls(issue.description)) urls.add(url)
    for (const comment of comments) for (const url of extractUploadUrls(comment.body)) urls.add(url)
    const queue = [...urls]
    let probed = 0
    const workers = Array.from({ length: 6 }, async () => {
      while (queue.length > 0) {
        const url = queue.shift()!
        try {
          const size = await options.probeAssetSize!(url)
          if (size !== null) assetSizes[url] = size
        } catch {
          // unknown size
        }
        probed += 1
        if (probed % 25 === 0) await progress(`assets`, probed, urls.size)
      }
    })
    await Promise.all(workers)
    await progress(`assets`, urls.size, urls.size)
  }

  return {
    version: 1,
    fetchedAt: (options.now?.() ?? new Date()).toISOString(),
    organization: who.organization,
    viewer: who.viewer,
    teams: meta.teams.nodes,
    states: meta.workflowStates.nodes.map((node) => ({
      id: node.id,
      name: node.name,
      type: node.type,
      color: node.color,
      position: node.position,
      teamId: node.team?.id ?? ``,
    })),
    labels,
    users: meta.users.nodes,
    projects: meta.projects.nodes.map((node) => ({
      id: node.id,
      name: node.name,
      teamIds: node.teams.nodes.map((team) => team.id),
    })),
    issues,
    comments,
    relations,
    assetSizes,
  }
}
