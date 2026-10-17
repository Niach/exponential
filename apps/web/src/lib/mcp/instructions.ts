// EXP-637: the MCP server's `instructions` field — the one piece of guidance
// every client loads up front, now that both Claude Code and Codex defer tool
// definitions behind tool search and only names + instructions are present at
// session start.
//
// Budget (locked by context-budget.test.ts): the FIRST paragraph must stand on
// its own inside 512 chars, because Codex reads only that much. Claude Code
// truncates at 2KB, so the whole string stays under 2000 chars.
//
// EXP-1291 (Threads v2): one session verb (`exponential_sessions_send`), one
// Guide per thread (`exponential_guide_write`), people are pinged by an
// `@email` mention in a message (no notify tool). The close-out wording is
// per caller (gates.ts): an unattended session ends itself LAST, an attended
// one only when asked, a team agent never.
//
// FEED-21: the report-bug paragraph follows the tool's EXP-496 gate (the
// instance has a feedback widget, i.e. cloud) — `reportBug` is per-instance,
// not per-caller, so it is a plain flag next to the gates.

export function mcpServerInstructions(gates: {
  /** The caller runs inside a session of its own. */
  inSession: boolean
  /** That session is a child or another session spawned it. */
  unattended: boolean
  /** That session's root is a team agent. */
  agent: boolean
  reportBug: boolean
}): string {
  const paragraphs = [
    // EXP-707 (theme D): status changes are AUTOMATIC — PR open/merge apply
    // the team's configured status automation; set one only when asked.
    `Exponential is this team's issue tracker and agent layer: issues on boards; sessions (threads and team agents) with their PRs and Guide. In a coding session: exponential_issues_get, exponential_comments_list, implement, commit and push, exponential_guide_write, then exponential_pr_open. Status changes are automatic. Search for exponential_* tools for boards, members, attachments, actions, sessions, devices, repos and teams.`,
    `exponential_sessions_send is the one session verb: sessionId messages a session, parentId starts a child, neither starts a thread. exponential_sessions_get (no id) reads your thread. Ping a person with @email in a message. PR tools take pullRequestId, issueId or repositoryId plus prNumber, default your session's newest open PR; merge a tree root first. Merging your own PR never ends your session.`,
    // EXP-792: the one registry. An agent asked to "add the Linear MCP"
    // would otherwise write a repo .mcp.json the launcher never reads.
    `Team MCP servers (Linear, Sentry...) live in exponential_mcp_servers_*, not a repo .mcp.json.`,
  ]
  if (gates.reportBug) {
    paragraphs.push(
      `When Exponential ITSELF misbehaves (a tool result contradicting its docs, a dropped start, a sync glitch), file it with exponential_report_bug. It reaches Exponential's developers, not the user's project.`
    )
  }
  if (gates.inSession && gates.agent) {
    paragraphs.push(
      `You are a team agent: you never end. Work arrives as messages and trigger firings; start threads for it with exponential_sessions_send and answer in your reply.`
    )
  }
  if (gates.inSession && !gates.agent && gates.unattended) {
    paragraphs.push(
      `This session is unattended (another session started it). Finish with exponential_sessions_end LAST, worktree clean: its summary (finished, stopped for a human, or changed nothing) is your final message and your parent's report. Never wait for replies; a question only your starter can answer goes to your parent session with exponential_sessions_send, then stop.`
    )
  }
  if (gates.inSession && !gates.agent && !gates.unattended) {
    paragraphs.push(
      `exponential_sessions_end ends this session; call it only when the person asks you to end the session.`
    )
  }
  // EXP-1280: plan mode is Exponential's (a card + the pr_open gate).
  if (gates.inSession) {
    paragraphs.push(
      `Plan mode: post the plan with exponential_plan_propose, then stop. Ask a person a choice with exponential_sessions_ask.`
    )
  }
  // EXP-879/1251: LAST, so it is what a truncating client keeps least — but
  // present, because a session never asked for its Guide never writes one.
  if (gates.inSession && !gates.agent) {
    paragraphs.push(
      `Close out with a Guide, never a chat summary: exponential_guide_write files per topic 2-3 sentences of GFM plus the files it touched ('Summary' first; sections together cover every changed file) and screenshots (one label each). The Guide IS the body of every PR in your thread: write it and screenshot every visible change BEFORE exponential_pr_open.`
    )
  }
  return paragraphs.join(`\n\n`)
}

/** The full variant — what the context budget measures. */
export const MCP_SERVER_INSTRUCTIONS = mcpServerInstructions({
  inSession: true,
  unattended: true,
  agent: false,
  reportBug: true,
})
