// EXP-637: the MCP server's `instructions` field — the one piece of guidance
// every client loads up front, now that both Claude Code and Codex defer tool
// definitions behind tool search and only names + instructions are present at
// session start.
//
// Budget (locked by context-budget.test.ts): the FIRST paragraph must stand on
// its own inside 512 chars, because Codex reads only that much. Claude Code
// truncates at 2KB, so the whole string stays under 2000 chars.
//
// EXP-1291: one session verb (`exponential_sessions_send`);
// S-3: `exponential_show` replaced the Guide; people are pinged by an
// `@email` mention in a message (no notify tool). The close-out wording is
// per caller (gates.ts): an unattended session ends itself LAST, an attended
// one only when asked, a team agent never.
//
// FEED-21: the report-bug paragraph follows the tool's EXP-496 gate (the
// instance has a feedback widget, i.e. cloud) — `reportBug` is per-instance,
// not per-caller, so it is a plain flag next to the gates.

import { DELETE_INSTRUCTIONS_LINE } from "@exp/emcp/deletes"

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
    // NEW-4 → EXP-1657: one tool per noun (+ 2 verbs), each with an `op`;
    // #VAPP-293: a noun's reads and its writes (`_write`) are two tools.
    // Statuses change AUTOMATICALLY on PR open/merge; set one only when asked.
    `Exponential is this team's issue tracker and agent layer: issues on boards; sessions (roots, children, team agents) with PRs and results. Each noun is a read tool plus a _write twin, each taking op plus that op's fields (exponential_issues {op: "get", id: "EXP-12"}). Coding session: issues get, comments list, implement, commit, push, screenshots as results, exponential_pr_write {op: "open"}, then end your turn. Statuses move automatically. Search for exponential_* tools (devices, events, people).`,
    // S-3: a launched session (the header) also carries the runbook, which
    // says this at length; only an outside client needs the verb spelled out.
    ...(gates.inSession
      ? [`Ping a person with @email.`]
      : [
          `exponential_sessions_send is the one session verb: sessionId messages a session (stop: true interrupts it), agentId a team agent (your conversation), parentId starts a child, neither starts a new root session. exponential_sessions {op: "get"} (no id) reads your tree. Ping a person with @email. exponential_pr ops take pullRequestId, issueId or repositoryId + prNumber (default: your newest open PR); merge a tree root first.`,
        ]),
    // EXP-792: the one registry. An agent asked to "add the Linear MCP"
    // would otherwise write a repo .mcp.json the launcher never reads.
    // #EXP-1798: a delete names one id the user gave (deletes.json).
    `Team MCP servers live in exponential_mcp_servers, never .mcp.json. ${DELETE_INSTRUCTIONS_LINE}`,
  ]
  if (gates.reportBug) {
    paragraphs.push(
      `When Exponential ITSELF misbehaves (a tool contradicting its docs, a dropped start, a sync glitch), file it with exponential_session_self {op: "report_bug"}: it reaches Exponential's developers.`
    )
  }
  if (gates.inSession && gates.agent) {
    paragraphs.push(
      `You are a team agent: you never end. Work arrives as messages, schedule ticks and events (exponential_events_write {op: "emit"} posts one); start root sessions with exponential_sessions_send (they report back here). Speak only when a person needs to know or decide, else call exponential_session_self {op: "quiet"} and end your turn with no text at all (never \`Noted.\` or \`Nothing new.\`); update your card (exponential_show replace: true) rather than posting again.`
    )
  }
  // #VAPP-293: a session's end stays on the read tool's name (first-party
  // callers may still send it there): a desktop gateway released before the
  // `_write` twin seals only `exponential_sessions` end summaries.
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
    paragraphs.push(LINKS_SENTENCE)
  }
  // S-3: LAST, so it is what a truncating client keeps least; the PR body
  // is server-written (a summary + the pictures by platform).
  if (gates.inSession && !gates.agent) {
    paragraphs.push(
      `Show your work as results: exponential_show with pictures (label = platform-prefixed: 'web …', 'ios …') before exponential_pr_write {op: "open"}; the PR body is written for you.`
    )
  }
  return paragraphs.join(`\n\n`)
}

/** S-3: the one sentence every launched session gets about showing. */
export const SHOW_SENTENCE = `Show, don't tell: results, questions (ask) and plans (plan) go in ONE exponential_show card per turn; replace: true updates it. Never a Card root (the chat draws the card); text only = a plain message.`

/** #EXP-1674: how an agent links into the app, in prose and in cards. */
export const LINKS_SENTENCE = `Link inside the app with ~/: [text](~/issues/EXP-12), ~/sessions/<id>, ~/sessions/<id>/plan/<messageId>, ~/boards/<slug>, ~/settings/<id>, ~/inbox, ~/apps/<slug>/<screen>?k=v (exponential_apps get lists them); elsewhere use a reply's url.`

/** The full variant — what the context budget measures. */
export const MCP_SERVER_INSTRUCTIONS = mcpServerInstructions({
  inSession: true,
  unattended: true,
  agent: false,
  reportBug: true,
})
