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
export const MCP_SERVER_INSTRUCTIONS = `Exponential is this team's issue tracker: issues on boards with comments, labels and the PRs that close them. In a coding session the flow is exponential_issues_get, exponential_comments_list, implement, commit and push, exponential_pr_open, then exponential_sessions_end last. Never set an issue to 'in_review' yourself; PR tools move issues. Search for exponential_* tools for boards, labels, statuses, members, attachments, notifications, actions, automations, sessions, devices, helpdesk, repos and teams.

exponential_sessions_end only works inside a session started by the Exponential launcher, and it is how a run reports back: a one-paragraph summary plus outcome 'done' (the PR is open or the work is complete), 'blocked' (you stopped and a human is needed) or 'no_changes' (nothing needed changing). Leave the worktree clean before you call it. It ends the session only when an automation started the run; a run a person started stays open afterwards so they can reply, so keep answering their follow-ups. Merging your own pull request never ends your session. exponential_pr_open takes 'issueId' for one issue, 'issueIds' plus 'head' for one combined PR over several, or 'repositoryId' plus 'head' for a chore PR with no issue at all. exponential_pr_merge mirrors that: 'issueId'/'issueIds', or 'repositoryId' plus 'prNumber'. If a merge is refused because the base is stale, call exponential_pr_retarget, rebase onto the new base, force-push with --force-with-lease, then merge again.`
