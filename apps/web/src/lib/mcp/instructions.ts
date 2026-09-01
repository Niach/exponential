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
}): string {
  const paragraphs = [
    // EXP-707 (theme D): status changes are AUTOMATIC — PR open/merge apply
    // the team's configured status automation, and a team configured to "do
    // nothing" means exactly that (the agent never compensates). Direct
    // status writes remain for one case only: the user explicitly asks.
    `Exponential is this team's issue tracker: issues on boards with comments, labels and the PRs that close them. In a coding session the flow is exponential_issues_get, exponential_comments_list, implement, commit and push, then exponential_pr_open. Status changes are automatic (PR tools apply the team's automation); set one only if asked. Search for exponential_* tools for boards, labels, statuses, members, attachments, notifications, actions, automations, sessions, devices, helpdesk, repos and teams.`,
    `exponential_pr_open takes 'issueId' for one issue, 'issueIds' plus 'head' for one combined PR over several, or 'repositoryId' plus 'head' for a chore PR with no issue at all. exponential_pr_merge mirrors that: 'issueId'/'issueIds', or 'repositoryId' plus 'prNumber'. Merging your own pull request never ends your session. If a merge is refused because the base is stale, call exponential_pr_retarget, rebase onto the new base, force-push with --force-with-lease, then merge again.`,
  ]
  if (gates.reportBug) {
    paragraphs.push(
      `When Exponential ITSELF misbehaves while you work — a tool result that contradicts its docs, a dropped remote start, a sync or UI glitch — file it right then with exponential_report_bug. That tool reports bugs in Exponential to its developers; it is never for issues in the user's own project.`
    )
  }
  if (gates.sessionsEnd) {
    // EXP-700: only a run another run started can ask its starter — the
    // exception rides the same paragraph, and askParent implies sessionsEnd.
    paragraphs.push(
      `This run is unattended (an automation or another agent started it). When you are done, call exponential_sessions_end LAST with a one-paragraph summary of what you did — whether you finished, stopped for a human or changed nothing; leave the worktree clean first. That call ends the run; nobody is watching, so do not wait for replies.` +
        (gates.askParent
          ? ` One exception: blocked on something only your starter knows, call exponential_sessions_ask_parent, then stop and wait — the answer arrives as a user message. Never wait silently without asking.`
          : ``)
    )
  }
  return paragraphs.join(`\n\n`)
}

/** The full variant — what the context budget measures. */
export const MCP_SERVER_INSTRUCTIONS = mcpServerInstructions({
  sessionsEnd: true,
  askParent: true,
  reportBug: true,
})
