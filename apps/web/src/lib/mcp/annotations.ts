// EXP-1153: the MCP tool annotations, ONE table for the whole surface.
//
// Every registered tool carries all three MCP hints as EXPLICIT booleans.
// Two readers depend on that:
//
// - Agents. FEED-25: a read without `readOnlyHint` costs a permission card
//   in claude's plan mode (a batch run once sat two hours on one for a plain
//   `attachments_get`); a write WITH it would skip the confirmation it
//   deserves.
// - Directory reviews. The ChatGPT/Codex plugin directory scans `tools/list`
//   and rejects a submission whose hints do not match the tool's behaviour
//   (developers.openai.com/plugins/plugin-guidelines#correct-annotation);
//   an absent hint counts as unset, not false. The rules there are strict
//   and this table follows them to the letter:
//
//   readOnlyHint    true  = fetches/lists/computes, changes nothing.
//   destructiveHint true  = deletes, OVERWRITES, cancels, revokes, or sends
//                           something that cannot be unsent. Being undoable
//                           does not make a write additive: only a pure
//                           append (create, add, subscribe, note) is false.
//   openWorldHint   true  = reaches beyond the bounded workspace: pushes to
//                           GitHub, emails a non-member, reports to a third
//                           party, or launches an agent that does any of it.
//                           A tool confined to the caller's teams is false
//                           even though the service is hosted.
//
// `registerExponentialTools` stamps the entry onto every registration, so a
// tool cannot ship without one (a missing name throws at registration and
// api-conventions.test.ts gates the table against the registered set).

export interface McpToolAnnotations {
  readonly readOnlyHint: boolean
  readonly destructiveHint: boolean
  readonly openWorldHint: boolean
}

/** Fetch/list/compute: changes nothing. */
const READ: McpToolAnnotations = {
  readOnlyHint: true,
  destructiveHint: false,
  openWorldHint: false,
}
/** Additive write inside the workspace: create, add, subscribe, note. */
const ADD: McpToolAnnotations = {
  readOnlyHint: false,
  destructiveHint: false,
  openWorldHint: false,
}
/** Overwrite, delete, cancel, revoke, end, or an irreversible send to
 *  members: inside the workspace, but not additive. */
const CHANGE: McpToolAnnotations = {
  readOnlyHint: false,
  destructiveHint: true,
  openWorldHint: false,
}
/** Additive, but it reaches outside the workspace: a PR pushed to GitHub,
 *  a run launched on a machine that will push code. */
const ADD_OPEN: McpToolAnnotations = {
  readOnlyHint: false,
  destructiveHint: false,
  openWorldHint: true,
}
/** Irreversible AND outside the workspace: a merge on GitHub, an email to a
 *  non-member, a report handed to a third party. */
const CHANGE_OPEN: McpToolAnnotations = {
  readOnlyHint: false,
  destructiveHint: true,
  openWorldHint: true,
}

export const TOOL_ANNOTATIONS = {
  // Teams
  exponential_teams_list: READ,
  exponential_teams_get: READ,
  exponential_teams_create: ADD,
  exponential_teams_update: CHANGE,

  // Boards
  exponential_boards_list: READ,
  exponential_boards_get: READ,
  exponential_boards_create: ADD,
  exponential_boards_update: CHANGE,
  exponential_boards_delete: CHANGE,
  exponential_boards_set_repository: CHANGE,

  // Issues
  exponential_issues_list: READ,
  exponential_issues_get: READ,
  exponential_issues_create: ADD,
  exponential_issues_update: CHANGE,
  exponential_issues_update_status: CHANGE,
  exponential_issues_delete: CHANGE,
  exponential_issues_subscribe: ADD,
  exponential_issues_unsubscribe: CHANGE,
  exponential_issues_pr_files: READ,
  // EXP-1153: the MCP App views — reads that a host renders as UI.
  exponential_board_view: READ,
  exponential_issue_view: READ,

  // Pull requests: every one of these acts on GitHub.
  exponential_pr_open: ADD_OPEN,
  exponential_pr_merge: CHANGE_OPEN,
  exponential_pr_retarget: CHANGE_OPEN,
  exponential_pr_update: CHANGE_OPEN,

  // Labels
  exponential_labels_list: READ,
  exponential_labels_get: READ,
  exponential_labels_create: ADD,
  exponential_labels_update: CHANGE,
  exponential_labels_delete: CHANGE,
  exponential_issue_labels_add: ADD,
  exponential_issue_labels_remove: CHANGE,

  // Relations
  exponential_issue_relations_add: ADD,
  exponential_issue_relations_remove: CHANGE,

  // Comments
  exponential_comments_list: READ,
  exponential_comments_create: ADD,
  exponential_comments_update: CHANGE,
  exponential_comments_delete: CHANGE,

  // Statuses
  exponential_statuses_list: READ,
  exponential_statuses_create: ADD,
  exponential_statuses_update: CHANGE,
  exponential_statuses_delete: CHANGE,

  // Members and invites (an email invite mails a non-member).
  exponential_members_list: READ,
  exponential_invites_list: READ,
  exponential_invites_create: CHANGE_OPEN,
  exponential_invites_revoke: CHANGE,

  // Notifications (a push cannot be unsent).
  exponential_notifications_list: READ,
  exponential_notifications_mark_read: CHANGE,
  exponential_notifications_send: CHANGE,

  // Repositories (registering one is bounded to the caller's GitHub grant).
  exponential_repositories_list: READ,
  exponential_repositories_add: ADD,
  exponential_repositories_branch_diff: READ,

  // Actions
  exponential_actions_list: READ,
  exponential_actions_create: ADD,
  exponential_actions_update: CHANGE,
  exponential_actions_delete: CHANGE,

  // Automations
  exponential_automations_list: READ,
  exponential_automations_create: ADD,
  exponential_automations_update: CHANGE,
  exponential_automations_toggle: CHANGE,
  exponential_automations_delete: CHANGE,

  // Attachments
  exponential_attachments_list: READ,
  exponential_attachments_get: READ,
  exponential_attachments_upload: ADD,
  exponential_attachments_delete: CHANGE,

  // Team MCP servers
  exponential_mcp_servers_list: READ,
  exponential_mcp_servers_add: ADD,
  exponential_mcp_servers_remove: CHANGE,

  // Devices and coding sessions. A start launches an agent on the user's
  // machine that pushes branches and opens PRs: open world.
  exponential_devices_list: READ,
  exponential_sessions_list: READ,
  exponential_sessions_get: READ,
  exponential_sessions_start: ADD_OPEN,
  exponential_sessions_message: ADD,
  exponential_sessions_kill: CHANGE,
  exponential_sessions_end: CHANGE,
  exponential_sessions_ask_parent: ADD,
  exponential_sessions_results: CHANGE,
  exponential_sessions_compact: CHANGE,

  // Helpdesk (a reply is emailed to the reporter, a non-member).
  exponential_helpdesk_threads_list: READ,
  exponential_helpdesk_threads_get: READ,
  exponential_helpdesk_reply: CHANGE_OPEN,
  exponential_helpdesk_note: ADD,
  exponential_helpdesk_close: CHANGE,
  exponential_helpdesk_reopen: CHANGE,
  exponential_helpdesk_escalate: ADD,

  // Workflows
  exponential_workflows_list: READ,
  exponential_workflows_get: READ,
  exponential_workflows_create: ADD,
  exponential_workflows_update: CHANGE,
  // A start launches node runs on the runner device that push code and
  // open PRs: open world. Pause/cancel end or freeze what is running.
  exponential_workflows_start: ADD_OPEN,
  exponential_workflows_pause: CHANGE,
  exponential_workflows_cancel: CHANGE,
  exponential_workflows_checkpoint: ADD,
  exponential_workflows_review_submit: ADD,
  exponential_workflows_request_upstream: ADD,

  // A report handed to Exponential's developers, a third party to the
  // user's own project.
  exponential_report_bug: CHANGE_OPEN,
} as const satisfies Record<string, McpToolAnnotations>

export type AnnotatedToolName = keyof typeof TOOL_ANNOTATIONS

/** The entry for a registration; a tool without one is a bug, not a
 *  default (an unset hint reads as "unknown" to every client). */
export function annotationsFor(name: string): McpToolAnnotations {
  const entry = (TOOL_ANNOTATIONS as Record<string, McpToolAnnotations>)[name]
  if (!entry) {
    throw new Error(
      `MCP tool ${name} has no entry in lib/mcp/annotations.ts — add one`
    )
  }
  return entry
}
