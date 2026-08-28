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
import type { McpToolGates } from "./gates"

export function mcpServerInstructions(
  gates: Pick<McpToolGates, `sessionsEnd`>
): string {
  const paragraphs = [
    `Exponential is this team's issue tracker: issues on boards with comments, labels and the PRs that close them. In a coding session the flow is exponential_issues_get, exponential_comments_list, implement, commit and push, then exponential_pr_open. Never set an issue to 'in_review' yourself; PR tools move issues. Search for exponential_* tools for boards, labels, statuses, members, attachments, notifications, actions, automations, sessions, devices, helpdesk, repos and teams.`,
    `exponential_pr_open takes 'issueId' for one issue, 'issueIds' plus 'head' for one combined PR over several, or 'repositoryId' plus 'head' for a chore PR with no issue at all. exponential_pr_merge mirrors that: 'issueId'/'issueIds', or 'repositoryId' plus 'prNumber'. Merging your own pull request never ends your session. If a merge is refused because the base is stale, call exponential_pr_retarget, rebase onto the new base, force-push with --force-with-lease, then merge again.`,
  ]
  if (gates.sessionsEnd) {
    paragraphs.push(
      `This run is unattended (an automation or another agent started it). When you are done, call exponential_sessions_end LAST with a one-paragraph summary and outcome 'done' (PR open or work complete), 'blocked' (you stopped and a human is needed) or 'no_changes'; leave the worktree clean first. That call ends the run; nobody is watching, so do not wait for replies.`
    )
  }
  return paragraphs.join(`\n\n`)
}

/** The full variant — what the context budget measures. */
export const MCP_SERVER_INSTRUCTIONS = mcpServerInstructions({
  sessionsEnd: true,
})
