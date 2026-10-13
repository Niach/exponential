import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure } from "@/lib/trpc"
import {
  agentFacetsSchema,
  agentTriggersSchema,
  boardIconSchema,
  codingSessionBlockedSchema,
  hexColorSchema,
  MESSAGE_BODY_MAX,
  messageCardSchema,
  sessionsSendInputSchema,
  sessionStatusSchema,
  type SessionsSendResult,
} from "@exp/db-schema/domain"

// EXP-1291 Threads v2: THE session router. A thread IS a root session; a
// team agent is a root of kind `agent`; children are sub-sessions. ONE
// entrypoint starts and messages them all: `sessions.send` (the composer,
// Start on an issue, an action, a trigger firing, an agent spawn, a
// follow-up). The database is the bus: a new session is a `queued` row
// naming its device, a message a `messages` row; the device picks both up
// off the `sessions` / `messages` shapes and reports back through the
// DEVICE procedures below (claim/report/reply/consume). Reads ride the
// shapes; tRPC returns ids + txids only.

const uuid = z.string().uuid()

const notYet = (): never => {
  throw new TRPCError({
    code: `NOT_IMPLEMENTED`,
    message: `sessions router: lands with EXP-1291's server half`,
  })
}

export const sessionsRouter = router({
  // ── people (and agents over MCP) ──────────────────────────────────────────

  /** THE entrypoint. → { sessionId, messageId, txid }. */
  send: authedProcedure
    .input(sessionsSendInputSchema)
    .mutation(async (): Promise<SessionsSendResult & { txid: number }> => notYet()),

  /** Interrupt the session's current turn (it stays live, goes idle). Every
   *  member may Stop a SHARED session; a private one only its starter. */
  stop: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .mutation(async (): Promise<{ txid: number }> => notYet()),

  /** End a session explicitly (status `ended`; agents refuse). Ending a
   *  ROOT ends its whole tree. */
  end: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .mutation(async (): Promise<{ txid: number }> => notYet()),

  /** Rename a thread (root) or a sub-session. */
  setTitle: authedProcedure
    .input(z.object({ sessionId: uuid, title: z.string().trim().max(255).nullable() }))
    .mutation(async (): Promise<{ txid: number }> => notYet()),

  /** Link issues to a thread (always lands on the ROOT; the header "+"). */
  linkIssues: authedProcedure
    .input(z.object({ sessionId: uuid, issueIds: z.array(uuid).min(1).max(30) }))
    .mutation(async (): Promise<{ txid: number }> => notYet()),

  unlinkIssue: authedProcedure
    .input(z.object({ sessionId: uuid, issueId: uuid }))
    .mutation(async (): Promise<{ txid: number }> => notYet()),

  /** Answer a `question` card (text) or decide a `plan` card IN the card. */
  answer: authedProcedure
    .input(
      z.object({
        messageId: uuid,
        answer: z.string().max(MESSAGE_BODY_MAX).optional(),
        decision: z.enum([`approved`, `rejected`]).optional(),
        feedback: z.string().max(MESSAGE_BODY_MAX).optional(),
      })
    )
    .mutation(async (): Promise<{ txid: number }> => notYet()),

  /** Revoke a person's own message the device has not consumed yet. */
  unqueue: authedProcedure
    .input(z.object({ messageId: uuid }))
    .mutation(async (): Promise<{ txid: number }> => notYet()),

  // ── team agents (Settings → Agents; owner writes, members read) ───────────
  // Create = `sessions.send({teamId, deviceId, account?, agent})`.

  /** The agent row's server-only facets (instructions) for the settings page. */
  getAgent: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .query(async (): Promise<{ sessionId: string; instructions: string }> => notYet()),

  updateAgent: authedProcedure
    .input(
      z.object({
        sessionId: uuid,
        name: agentFacetsSchema.shape.name.optional(),
        icon: boardIconSchema.optional(),
        color: hexColorSchema.optional(),
        instructions: agentFacetsSchema.shape.instructions.optional(),
        triggers: agentTriggersSchema.optional(),
        deviceId: z.string().min(1).max(128).optional(),
        account: z.string().min(1).max(64).nullable().optional(),
        model: z.string().max(64).nullable().optional(),
        sortOrder: z.number().finite().optional(),
      })
    )
    .mutation(async (): Promise<{ txid: number }> => notYet()),

  deleteAgent: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .mutation(async (): Promise<{ txid: number }> => notYet()),

  // ── the device (the daemon / desktop that runs the session) ───────────────

  /** The server-composed first prompt (fixtures/session-brief.json) and the
   *  system-append layers: runbook + team prompt + agent instructions. */
  brief: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .query(
      async (): Promise<{
        brief: string
        teamPrompt: string | null
        agentInstructions: string | null
        parentBranch: string | null
      }> => notYet()
    ),

  /** queued → running: the device took it (idempotent for its own device). */
  claim: authedProcedure
    .input(z.object({ sessionId: uuid, branch: z.string().max(255).optional() }))
    .mutation(async (): Promise<{ txid: number }> => notYet()),

  /** Device-written state: running ⇄ idle, busy, needs_input, blocked,
   *  branch, the CLI's auto-title; `ended` when the agent process is gone. */
  report: authedProcedure
    .input(
      z.object({
        sessionId: uuid,
        status: sessionStatusSchema.exclude([`queued`]).optional(),
        busy: z.boolean().optional(),
        needsInput: z.boolean().optional(),
        blocked: codingSessionBlockedSchema.nullable().optional(),
        branch: z.string().max(255).optional(),
        title: z.string().max(255).optional(),
      })
    )
    .mutation(async (): Promise<{ txid: number }> => notYet()),

  /** The agent's side of the conversation: a turn's FINAL reply text and/or
   *  a card (question, plan, subagent, picture). */
  reply: authedProcedure
    .input(
      z.object({
        sessionId: uuid,
        body: z.string().max(MESSAGE_BODY_MAX).default(``),
        card: messageCardSchema.optional(),
      })
    )
    .mutation(async (): Promise<{ messageId: string; txid: number }> => notYet()),

  /** The device delivered these person messages into the agent. */
  consume: authedProcedure
    .input(z.object({ sessionId: uuid, messageIds: z.array(uuid).min(1).max(100) }))
    .mutation(async (): Promise<{ txid: number }> => notYet()),
})

// EXP-1291: the PR model (`pull_requests` + `pull_request_issues`, synced).
// PR state is written by `pr_open`, the merge paths, retarget and the GitHub
// webhook/poller; people act on a PR through these.
export const pullRequestsRouter = router({
  /** Squash-merge; `stack` = merge THROUGH this member of a native stack. */
  merge: authedProcedure
    .input(z.object({ pullRequestId: uuid, stack: z.boolean().optional() }))
    .mutation(
      async (): Promise<{ merged: boolean; queued?: boolean; error?: string; txid: number }> =>
        notYet()
    ),

  close: authedProcedure
    .input(z.object({ pullRequestId: uuid }))
    .mutation(async (): Promise<{ txid: number }> => notYet()),

  /** The PR's changed files (the Guide's Changes rows). */
  files: authedProcedure
    .input(z.object({ pullRequestId: uuid }))
    .query(
      async (): Promise<{
        files: { path: string; additions: number; deletions: number; patch: string | null }[]
      }> => notYet()
    ),
})
