// EXP-1183 — the pure half of the issue face (issue-detail-view.tsx): which
// machines "Start coding" may offer, the comment thread's order, and the
// small formatting the view repeats. Kept out of the component so the rules
// are unit-tested (issue-detail-logic.test.ts).

/** One `exponential_devices_list` row — only what the start menu reads. */
export interface StartDeviceRow {
  deviceId: string
  label?: string | null
  kind?: string | null
  online?: boolean
  agents?: string[] | null
  isDefault?: boolean
  owner?: { name?: string | null } | null
}

/** One pickable launch: a device, and the agent on it (null = the device's
 *  own default, for a machine that reports none). */
export interface StartTarget {
  key: string
  deviceId: string
  deviceLabel: string
  agent: string | null
  label: string
}

const AGENT_LABEL: Record<string, string> = { claude: `Claude`, codex: `Codex` }

export function agentLabel(agent: string): string {
  return AGENT_LABEL[agent] ?? agent.charAt(0).toUpperCase() + agent.slice(1)
}

/** ONLINE devices only (offline = sessions_start refuses), the default one
 *  first, then by name; one target per runnable agent. */
export function startTargets(rows: readonly StartDeviceRow[]): StartTarget[] {
  const online = rows
    .filter((row) => row.online && row.deviceId)
    .sort(
      (a, b) =>
        Number(Boolean(b.isDefault)) - Number(Boolean(a.isDefault)) ||
        deviceName(a).localeCompare(deviceName(b))
    )
  return online.flatMap((row): StartTarget[] => {
    const name = deviceName(row)
    const agents = [...new Set(row.agents ?? [])]
    if (agents.length === 0) {
      return [{ key: row.deviceId, deviceId: row.deviceId, deviceLabel: name, agent: null, label: name }]
    }
    return agents.map((agent) => ({
      key: `${row.deviceId}:${agent}`,
      deviceId: row.deviceId,
      deviceLabel: name,
      agent,
      label: `${agentLabel(agent)} on ${name}`,
    }))
  })
}

function deviceName(row: StartDeviceRow): string {
  const label = row.label?.trim() || `Device`
  return row.owner?.name ? `${label} (${row.owner.name})` : label
}

/** The `exponential_sessions_start` arguments for a target. */
export function startArgs(
  target: StartTarget,
  issueId: string
): Record<string, unknown> {
  return {
    deviceId: target.deviceId,
    issueId,
    ...(target.agent ? { agent: target.agent } : {}),
  }
}

export interface ThreadComment {
  id: string
  parentId: string | null
  createdAt: string
}

/** Comments thread ONE level (CLAUDE.md): roots oldest first, each followed
 *  by its replies oldest first. A reply whose root is outside the fetched
 *  window stands as a root. The tool returns newest first. */
export function threadComments<T extends ThreadComment>(
  comments: readonly T[]
): { comment: T; reply: boolean }[] {
  const sorted = [...comments].sort(
    (a, b) => new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime()
  )
  const ids = new Set(sorted.map((comment) => comment.id))
  const replies = new Map<string, T[]>()
  const roots: T[] = []
  for (const comment of sorted) {
    if (comment.parentId && ids.has(comment.parentId)) {
      const list = replies.get(comment.parentId) ?? []
      list.push(comment)
      replies.set(comment.parentId, list)
    } else {
      roots.push(comment)
    }
  }
  return roots.flatMap((root) => [
    { comment: root, reply: false },
    ...(replies.get(root.id) ?? []).map((comment) => ({ comment, reply: true })),
  ])
}

/** The "via MCP" / "reporter" caption ×4 (EXP-741, SLOP-4). */
export function commentCaption(source: string | null | undefined): string | null {
  if (source === `mcp`) return `via MCP`
  if (source === `reporter`) return `reporter`
  return null
}

export function appOrigin(url: string | null | undefined): string | undefined {
  if (!url) return undefined
  try {
    return new URL(url).origin
  } catch {
    return undefined
  }
}

export function formatDate(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return ``
  return date.toLocaleString(undefined, {
    month: `short`,
    day: `numeric`,
    hour: `2-digit`,
    minute: `2-digit`,
  })
}
