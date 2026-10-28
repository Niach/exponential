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
    // NEW-4: 17 tools, one per noun (+ 3 verbs), each with an `op`; statuses change
    // AUTOMATICALLY on PR open/merge; set one only when asked.
    `Exponential is this team's issue tracker and agent layer: issues on boards; sessions (threads, team agents) with PRs and a Guide. 17 tools, one per noun, each taking op plus that op's fields (exponential_issues {op: "get", id: "EXP-12"}; its description lists the ops). Coding session: issues get, comments list, implement, commit, push, exponential_guide_write, exponential_pr {op: "open"}. Statuses move automatically. Search for exponential_* tools (devices, events, people).`,
    // S-3: a launched session (the header) also carries the runbook, which
    // says this at length; only an outside client needs the verb spelled out.
    ...(gates.inSession
      ? [`Ping a person with @email.`]
      : [
          `exponential_sessions_send is the one session verb: sessionId messages a session (stop: true interrupts it), agentId a team agent (your conversation), parentId starts a child, neither starts a thread. exponential_sessions {op: "get"} (no id) reads your thread. Ping a person with @email. exponential_pr ops take pullRequestId, issueId or repositoryId + prNumber (default: your newest open PR); merge a tree root first.`,
        ]),
    // EXP-792: the one registry. An agent asked to "add the Linear MCP"
    // would otherwise write a repo .mcp.json the launcher never reads.
    `Team MCP servers live in exponential_team_config (mcp_servers_*), never .mcp.json.`,
  ]
  if (gates.reportBug) {
    paragraphs.push(
      `When Exponential ITSELF misbehaves (a tool contradicting its docs, a dropped start, a sync glitch), file it with exponential_session_self {op: "report_bug"}: it reaches Exponential's developers.`
    )
  }
  if (gates.inSession && gates.agent) {
    paragraphs.push(
      `You are a team agent: you never end. Work arrives as messages, schedule ticks and events (exponential_events {op: "emit"} posts one); start threads with exponential_sessions_send (they report back here). Speak only when a person needs to know or decide, else end your turn silently; update your card (exponential_show replace: true) rather than posting again.`
    )
  }
  if (gates.inSession && !gates.agent && gates.unattended) {
    paragraphs.push(
      `This session is unattended (another session started it). Finish with exponential_sessions {op: "end", summary} LAST, worktree clean: the summary (finished, stopped for a human, or changed nothing) is your final message and your parent's report. Never wait or poll: end your turn, messages wake you.`
    )
  }
  if (gates.inSession && !gates.agent && !gates.unattended) {
    paragraphs.push(
      `exponential_sessions {op: "end"} ends this session; call it only when the person asks you to end the session. Never wait or poll: end your turn, messages wake you.`
    )
  }
  // S-3: plan mode is gone; results, questions and plans are ONE show card
  // (`exponential_show`, always loaded), edited in place.
  if (gates.inSession) {
    paragraphs.push(SHOW_SENTENCE)
  }
  // EXP-879/1251: LAST, so it is what a truncating client keeps least — but
  // present, because a session never asked for its Guide never writes one.
  if (gates.inSession && !gates.agent) {
    paragraphs.push(
      `Close out with a Guide, never a chat summary: exponential_guide_write files per topic 2-3 sentences of GFM plus the files it touched ('Summary' first; together they cover every changed file) and screenshots (one label each). The Guide IS every PR's body: write it (with screenshots) BEFORE exponential_pr {op: "open"}.`
    )
  }
  return paragraphs.join(`\n\n`)
}

/** S-3: the one sentence every launched session gets about showing. */
export const SHOW_SENTENCE = `Show, don't tell: results, questions (ask) and plans (plan) go in ONE exponential_show card per turn; replace: true updates it.`

/** The full variant — what the context budget measures. */
export const MCP_SERVER_INSTRUCTIONS = mcpServerInstructions({
  inSession: true,
  unattended: true,
  agent: false,
  reportBug: true,
})
