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

import { PLAN_CARD_COPY } from "@exp/domain-contract/plan-card"

export function mcpServerInstructions(gates: {
  /** The caller runs inside a session of its own. */
  inSession: boolean
  /** That session is a child or another session spawned it. */
  unattended: boolean
  /** That session's root is a team agent. */
  agent: boolean
  /** EXP-1280: that session is still planning (`sessions.plan_mode`). */
  planMode?: boolean
  /** EXP-1280: that session's host feeds its own cards back (a desktop/CLI
   *  from 0.14.73; `hostFeedsOwnCards` in gates.ts). False = the ask tools
   *  are not registered, so neither the ask nor the plan-mode gate is said.
   *  Absent = true. */
  cards?: boolean
  reportBug: boolean
}): string {
  const paragraphs = [
    // NEW-4: 16 tools, one per noun, each with an `op`; statuses change
    // AUTOMATICALLY on PR open/merge; set one only when asked.
    `Exponential is this team's issue tracker and agent layer: issues on boards; sessions (threads, team agents) with PRs and a Guide. 16 tools, one per noun, each taking op plus that op's fields (exponential_issues {op: "get", id: "EXP-12"}; its description lists the ops). Coding session: issues get, comments list, implement, commit, push, exponential_guide_write, exponential_pr {op: "open"}. Statuses move automatically. Search for exponential_* tools (devices, events, people).`,
    `exponential_sessions_send is the one session verb: sessionId messages a session (stop: true interrupts it), agentId a team agent (your conversation), parentId starts a child, neither starts a thread. exponential_sessions {op: "get"} (no id) reads your thread. Ping a person with @email. exponential_pr ops take pullRequestId, issueId or repositoryId + prNumber (default: your newest open PR); merge a tree root first.`,
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
      `You are a team agent: you never end. Work arrives as messages, schedule ticks and events (exponential_events {op: "emit"} posts one); start threads with exponential_sessions_send (they report back here); answer in your reply.`
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
  // EXP-1280: plan mode is Exponential's (a card + the pr open gate): a
  // planning session gets THE gate sentence (plan-card.json `planModeGate`,
  // the device's plan-mode line says the same), every session the ask —
  // unless the host cannot feed a card's answer back (`cards === false`).
  if (gates.inSession && gates.cards !== false) {
    const ask = `Ask a person with exponential_session_self {op: "ask"}.`
    paragraphs.push(gates.planMode === true ? `${PLAN_CARD_COPY.planModeGate} ${ask}` : ask)
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

/** The full variant — what the context budget measures. */
export const MCP_SERVER_INSTRUCTIONS = mcpServerInstructions({
  inSession: true,
  unattended: true,
  agent: false,
  planMode: true,
  reportBug: true,
})
