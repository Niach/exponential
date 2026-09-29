// EXP-637: the MCP server's `instructions` field — the one piece of guidance
// every client loads up front, now that both Claude Code and Codex defer tool
// definitions behind tool search and only names + instructions are present at
// session start.
//
// Budget (locked by context-budget.test.ts): the FIRST paragraph must stand on
// its own inside 512 chars, because Codex reads only that much. Claude Code
// truncates at 2KB, so the whole string stays under 2000 chars. It names the
// always-loaded tools by their exact names and tells the agent that everything
// else is found by searching for `exponential_*`.
//
// EXP-679: the close-out paragraph is per-caller, like the tool itself — only
// an unattended run is told about exponential_sessions_end, because only an
// unattended run has it registered.
//
// FEED-21: the report-bug paragraph follows the tool's own EXP-496 gate (the
// instance has a feedback widget, i.e. cloud) — a deferred tool description is
// invisible until an agent already thinks to search for it, so the trigger has
// to live here. `reportBug` is per-instance, not per-caller, which is why it
// is a plain flag next to the gates instead of a McpToolGates field.

export function mcpServerInstructions(gates: {
  sessionsEnd: boolean
  askParent: boolean
  reportBug: boolean
  // EXP-879: the caller runs inside a session of its own, so it has a run to
  // publish screenshots on. Same per-caller rule as the close-out paragraph.
  sessionResults: boolean
}): string {
  const paragraphs = [
    // EXP-707 (theme D): status changes are AUTOMATIC — PR open/merge apply
    // the team's configured status automation, and a team configured to "do
    // nothing" means exactly that (the agent never compensates). Direct
    // status writes remain for one case only: the user explicitly asks.
    `Exponential is this team's issue tracker: issues on boards with comments, labels and the PRs that close them. In a coding session the flow is exponential_issues_get, exponential_comments_list, implement, commit and push, then exponential_pr_open. Status changes are automatic (PR tools apply the team's automation); set one only if asked. Search for exponential_* tools for boards, members, attachments, notifications, actions, automations, sessions, devices, helpdesk, repos and teams.`,
    `exponential_pr_open takes 'issueId', 'issueIds' plus 'head' for one combined PR, or 'repositoryId' plus 'head' for an issue-less chore PR; exponential_pr_merge mirrors it ('repositoryId' plus 'prNumber'). Merging your own PR never ends your session. A merge refused for a stale base: exponential_pr_retarget, rebase, force-push with --force-with-lease, merge again.`,
    // EXP-792: the one registry. An agent asked to "add the Linear MCP"
    // would otherwise write a repo .mcp.json the launcher never reads.
    `Team MCP servers (Linear, Sentry...) live in exponential_mcp_servers_*, not a repo .mcp.json.`,
  ]
  if (gates.reportBug) {
    paragraphs.push(
      `When Exponential ITSELF misbehaves (a tool result contradicting its docs, a dropped remote start, a sync or UI glitch), file it right then with exponential_report_bug. It reaches Exponential's developers, never the user's project.`
    )
  }
  if (gates.sessionsEnd) {
    // EXP-700: only a run another run started can ask its starter — the
    // exception rides the same paragraph, and askParent implies sessionsEnd.
    paragraphs.push(
      `This run is unattended (an automation or another agent started it). Finish with exponential_sessions_end LAST: a one-paragraph summary (finished, stopped for a human, or changed nothing), worktree clean. It ends the run; nobody is watching, so never wait for replies.` +
        (gates.askParent
          ? ` Blocked on something only your starter knows: call exponential_sessions_ask_parent, then stop and wait for the answer (a user message). Never wait silently.`
          : ``)
    )
  }
  // EXP-1089: an attended run (a person's chat, a planner run pressed from
  // the workflow page) may ask its owner too — the tool is registered for
  // every run now, and the paragraph above only rides the close-out.
  if (gates.askParent && !gates.sessionsEnd) {
    paragraphs.push(
      `A question only the person who owns this run can answer: exponential_sessions_ask_parent with to 'user' notifies them and parks this run as needing input; stop and wait, the answer arrives as a user message.`
    )
  }
  // EXP-879: LAST, so it is the thing a truncating client keeps least — but
  // present, because a run that is never asked for a picture never takes one.
  // EXP-933: the same tool files the run's REPORT (the close-out lives on the
  // issue's Results, not in chat), and the notify rule rides here because
  // only a run has an issue whose Results a notification can open.
  // EXP-1144: the screenshot half is a RULE with a deadline (before the PR),
  // not a description — the descriptive line was read and skipped.
  if (gates.sessionResults) {
    paragraphs.push(
      `Close out with a report, never a chat summary: exponential_sessions_results files GFM text per topic ('Summary' first: what you did) over screenshots (one topic per screen, one label each) on the issue's Results; screenshot every visible change BEFORE exponential_pr_open. exponential_notifications_send pings a person (default you) when a long task ends or a decision waits; it opens them.`
    )
  }
  return paragraphs.join(`\n\n`)
}

/** The full variant — what the context budget measures. */
export const MCP_SERVER_INSTRUCTIONS = mcpServerInstructions({
  sessionsEnd: true,
  askParent: true,
  reportBug: true,
  sessionResults: true,
})
