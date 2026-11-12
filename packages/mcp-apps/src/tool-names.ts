// #VAPP-292 — the views were written against the per-operation tools
// (`exponential_issues_get`, EXP-988); NEW-4 folded them into 22 noun tools
// that take an `op`. A host only forwards a view's tools/call to a tool the
// server LISTS, so `exponential_issues_get` was refused and an issue row
// opened nothing. Every call the views make goes through `nounCall`: an
// op-named tool becomes its noun + `op`, the two retired session verbs
// become `exponential_sessions_send`. A noun or verb name passes as is.

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
  if (!route) return { name, args }
  return { name: `${PREFIX}${route[0]}`, args: { op: route[1], ...args } }
}
