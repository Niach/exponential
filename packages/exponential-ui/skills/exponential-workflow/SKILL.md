---
name: exponential-workflow
description: "Work in Exponential the way its sessions do: the noun tools and their _write twins, a coding session from issue to pull request, the session tree and wake-ups (never poll), ending and reporting, showing results and asking through one card, and the same UI on every platform. Use when acting on Exponential issues, sessions or pull requests over MCP."
---

# The Exponential workflow

> Generated from docs/spec/workflow.md (genUI 0.21.0) by packages/exponential-ui/scripts/generate-skills.ts — do not edit.

Exponential is a team's issue tracker and agent layer: issues on boards, sessions (roots, children, team agents) with pull requests and results. This is how an agent works in it, over MCP or inside a session.

## The tools

- Each noun is a read tool plus a `_write` twin, each taking `op` plus that op's fields flat beside it: `exponential_issues {op: "get", id: "EXP-12"}`, `exponential_issues_write {op: "update", id: "EXP-12", priority: "high"}`. A tool's description lists its ops.
- Two verbs stand alone: `exponential_sessions_send` (start or message a session) and `exponential_show` (draw one genUI card).
- Resolve ids before writing: an issue by its identifier (`EXP-12`), a board by its slug, a person by email (`exponential_people {op: "list"}`).
- Delete only an id the person named, one per call; never act on a list result.
- Team MCP servers live in `exponential_mcp_servers`, never in a repository `.mcp.json`.

## A coding session

1. Read the issue and its comments (`exponential_issues {op: "get"}`, `exponential_comments {op: "list"}`); the parent issue holds the decisions.
2. Implement on the session's own branch, commit, push.
3. Show the work as results: `exponential_show` with pictures, one per changed platform (labels start with the platform: `web …`, `ios …`, `android …`, `desktop …`).
4. Open the pull request with `exponential_pr_write {op: "open"}` (the body is written for you), then end your turn. The merge train lands it; CI and the merge wake you.
5. Statuses move by themselves on PR open and merge; set one only when asked.

Work out of scope, or a bug you will not fix now, is a new issue (`exponential_issues_write {op: "create"}`), never a wider pull request.

## Sessions

- One tree per conversation: `exponential_sessions_send {parentId}` starts a child that reports back; `{sessionId}` messages a session (`stop: true` interrupts its turn); neither starts a new root on a device (`deviceId`).
- Never wait, sleep or poll. End your turn; messages wake you. To wait on something, `wakeOn`: a session, `pr:N` (its CI settled), `pr:N:closed`, `issue:X[:status]`, `at:<ISO time>`.
- A session someone else started (a parent, an agent, a trigger) finishes with `exponential_sessions {op: "end", summary}` as its LAST call, worktree clean; the summary is its parent's report. A session a person started ends only when asked.

## Talking

- Your final text of a turn is your message. Speak when a person needs to know or decide; otherwise stay quiet (`exponential_session_self {op: "quiet"}`).
- Show, don't tell: results, questions (`ask`) and plans (`plan`) go in ONE `exponential_show` card per turn; `replace: true` updates it. After an ask or a plan, end your turn: the answer arrives as the next message.
- Ping a person with `@<email>`; name an issue as `#EXP-12`; link inside the app with `~/` (`~/issues/EXP-12`, `~/sessions/<id>`).

## One UI on every platform

Web, desktop, iOS and Android look exactly the same. Build screens and cards from the existing genUI components (the `exponential-genui` skill; `exponential_apps {op: "catalog"}` lists them); a new low-level component goes into genUI itself, never into one app. Nothing that already exists in the catalogue is built a second time without approval.
