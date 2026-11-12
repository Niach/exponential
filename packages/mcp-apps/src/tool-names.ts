// #VAPP-292 — the views were written against the per-operation tools
// (`exponential_issues_get`, EXP-988); NEW-4 folded them into 22 noun tools
// that take an `op`. A host only forwards a view's tools/call to a tool the
// server LISTS, so `exponential_issues_get` was refused and an issue row
// opened nothing. Every call the views make goes through `nounCall`: an
// op-named tool becomes its noun + `op`, the two retired session verbs
// become `exponential_sessions_send`. A noun or verb name passes as is;
// #VAPP-293: a write op lands on the noun's `_write` twin.

/** Per-operation name (without `exponential_`) → [noun, op]. */
const OPS: Readonly<Record<string, readonly [string, string]>> = {
  issues_list: [`issues`, `list`],
  issues_get: [`issues`, `get`],
  issues_show: [`issues`, `show`],
  issues_update: [`issues`, `update`],
  issues_update_status: [`issues`, `update_status`],
  boards_get: [`boards`, `get`],
  boards_list: [`boards`, `list`],
  members_list: [`people`, `list`],
  comments_list: [`comments`, `list`],
  comments_create: [`comments`, `create`],
  attachments_get: [`attachments`, `get`],
  attachments_list: [`attachments`, `list`],
  devices_list: [`devices`, `list`],
  devices_account_login: [`devices`, `account_login`],
  notifications_list: [`notifications`, `list`],
  notifications_mark_read: [`notifications`, `mark_read`],
  sessions_get: [`sessions`, `get`],
  sessions_list: [`sessions`, `list`],
  pr_merge: [`pr`, `merge`],
  pr_files: [`pr`, `files`],
  issues_pr_files: [`pr`, `files`],
  teams_get: [`teams`, `get`],
}

const PREFIX = `exponential_`

/** #VAPP-293: each noun's READ ops; the server registers every other op on
 *  the noun's `_write` twin (an OAuth caller such as Claude reaches a write
 *  only there). Pinned to the server's split by apps/web's
 *  api-conventions.test.ts. `session_self` has no reads and keeps its name. */
export const NOUN_READ_OPS: Readonly<Record<string, readonly string[]>> = {
  issues: [`list`, `get`, `show`, `trash`, `activity`],
  comments: [`list`],
  pr: [`files`],
  boards: [`list`, `get`],
  labels: [`list`, `get`],
  statuses: [`list`],
  attachments: [`list`, `get`],
  teams: [`list`, `get`],
  people: [`list`, `invites_list`],
  repositories: [`list`],
  devices: [`list`],
  agents: [`list`, `get`],
  prompts: [`list`, `get`],
  skills: [`list`, `get`],
  mcp_servers: [`list`],
  sessions: [`get`, `list`],
  apps: [`list`, `get`, `catalog`, `records_list`, `records_history`],
  events: [`list`],
  notifications: [`list`],
}

/** A noun call on the half that serves its op: a write → `<noun>_write`. */
function onItsHalf(target: { name: string; args: Record<string, unknown> }): { name: string; args: Record<string, unknown> } {
  const reads = NOUN_READ_OPS[target.name.slice(PREFIX.length)]
  const op = target.args.op
  if (!reads || typeof op !== `string` || reads.includes(op)) return target
  return { name: `${target.name}_write`, args: target.args }
}

/** The tool a view's call reaches on the server, with its arguments. */
export function nounCall(
  name: string,
  args: Record<string, unknown>
): { name: string; args: Record<string, unknown> } {
  if (!name.startsWith(PREFIX)) return { name, args }
  const bare = name.slice(PREFIX.length)
  // The coding start and the steer message are sessions_send now.
  if (bare === `sessions_start`) {
    const { issueId, agent, ...rest } = args
    return {
      name: `${PREFIX}sessions_send`,
      args: {
        ...rest,
        ...(typeof issueId === `string` ? { issueIds: [issueId] } : {}),
        ...(typeof agent === `string` ? { cli: agent } : {}),
      },
    }
  }
  if (bare === `sessions_message`) {
    const { id, message, ...rest } = args
    return { name: `${PREFIX}sessions_send`, args: { ...rest, sessionId: id, text: message } }
  }
  const route = OPS[bare]
  if (!route) return onItsHalf({ name, args })
  return onItsHalf({ name: `${PREFIX}${route[0]}`, args: { op: route[1], ...args } })
}
