import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { contract } from "@exp/domain-contract"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { codingSessions } from "@/db/schema"
import {
  assertWorkspaceMember,
  getIssueWorkspaceContext,
} from "@/lib/workspace-membership"
import { resolveProjectRepository } from "@/lib/trpc/repositories"
import {
  getSteerRelayConfig,
  mintSteerTicket,
  relayGetDevices,
  relayPostKill,
  relayPostStart,
  type SteerDevice,
} from "@/lib/steer"

// Remote start + live terminal steer (masterplan §3.5). The web app is the
// only place that holds STEER_RELAY_SECRET: it mints short-lived HS256 relay
// tickets after checking the caller's workspace permission, and talks to the
// relay's secret-authed admin HTTP for device presence / remote start / kill.
// The relay itself never sees raw credentials and holds no DB — all
// authorization is decided here at mint time. STEER_RELAY_URL unset ⇒ the
// whole subsystem reports disabled and every proc degrades gracefully.

async function loadCodingSession(id: string) {
  const { db } = await import(`@/db/connection`)
  const [session] = await db
    .select()
    .from(codingSessions)
    .where(eq(codingSessions.id, id))
    .limit(1)
  if (!session) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Coding session not found`,
    })
  }
  return session
}

// The Start-coding dialog value sets (EXP-149). Blank effort is the explicit
// "CLI default" (omit --effort) — a per-client extra row, not a contract value.
const codingModelValues = contract.codingModel.values as [string, ...string[]]
const codingEffortValues = [``, ...contract.codingEffort.values] as [
  string,
  ...string[],
]

const mintTicketInput = z.discriminatedUnion(`kind`, [
  // Desktop device-presence socket (no sessionId yet).
  z.object({
    kind: z.literal(`control`),
    deviceLabel: z.string().max(255).optional(),
  }),
  // Desktop PTY publisher for a session it started.
  z.object({
    kind: z.literal(`publisher`),
    codingSessionId: z.string().uuid(),
  }),
  // Web/mobile watcher (perm view) or steerer (perm steer) of a session.
  z.object({
    kind: z.literal(`viewer`),
    codingSessionId: z.string().uuid(),
  }),
])

export const steerRouter = router({
  // Whether remote start + live steering is available on this instance —
  // enabled iff BOTH STEER_RELAY_URL and STEER_RELAY_SECRET are set. No relay
  // round-trip; clients poll this before dialing anything.
  config: authedProcedure.query(() => {
    const config = getSteerRelayConfig()
    return { enabled: config !== null, relayUrl: config?.url ?? null }
  }),

  // Mint a short-lived relay ticket (60s connect window) + the full ws(s)
  // dial URL. Relay disabled is a result, not an error, so desktop pollers
  // don't treat an unconfigured instance as a failure.
  mintTicket: authedProcedure
    .input(mintTicketInput)
    .mutation(async ({ ctx, input }) => {
      const config = getSteerRelayConfig()
      if (!config) return { disabled: true as const }
      const userId = ctx.session.user.id

      // Any authed user may register device presence for their own account.
      if (input.kind === `control`) {
        return mintSteerTicket(config, {
          kind: `control`,
          userId,
          deviceLabel: input.deviceLabel,
        })
      }

      const session = await loadCodingSession(input.codingSessionId)

      // Only the session owner's own desktop may publish its PTY.
      if (input.kind === `publisher`) {
        if (session.userId !== userId) {
          throw new TRPCError({
            code: `FORBIDDEN`,
            message: `Only the session owner can publish it`,
          })
        }
        return mintSteerTicket(config, {
          kind: `publisher`,
          userId,
          workspaceId: session.workspaceId,
          sessionId: session.id,
        })
      }

      // Viewers must be members of the session's workspace; workspace owners
      // may steer, and so may the session's own starter — plain members watch
      // (viewerPermFor).
      const member = await assertWorkspaceMember(userId, session.workspaceId)
      return mintSteerTicket(config, {
        kind: `viewer`,
        userId,
        workspaceId: session.workspaceId,
        sessionId: session.id,
        role: member.role,
        isSessionOwner: session.userId === userId,
        name: ctx.session.user.name || ctx.session.user.email,
      })
    }),

  // The caller's online desktops — powers the "Start on my desktop" picker.
  myDevices: authedProcedure.query(async ({ ctx }) => {
    const config = getSteerRelayConfig()
    if (!config) return { devices: [] as SteerDevice[] }
    return relayGetDevices(config, ctx.session.user.id)
  }),

  // Remote "Start on my desktop": route a start command to the chosen online
  // device's control socket via the relay. The optional launch options are the
  // client's Start-coding dialog choices (EXP-149) — validated against the
  // domain-contract value sets here (the relay is a dumb pipe); absent fields
  // mean desktop settings defaults with plan mode OFF.
  startSession: authedProcedure
    .input(
      z.object({
        issueId: z.string().uuid(),
        deviceId: z.string().min(1).max(128),
        model: z.enum(codingModelValues).optional(),
        effort: z.enum(codingEffortValues).optional(),
        ultracode: z.boolean().optional(),
        planMode: z.boolean().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const config = getSteerRelayConfig()
      if (!config) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `Remote start is not enabled on this instance`,
        })
      }
      const userId = ctx.session.user.id
      const issueCtx = await getIssueWorkspaceContext(input.issueId)
      await assertWorkspaceMember(userId, issueCtx.workspaceId)

      // The launcher can't do anything without a linked repo — fail before
      // waking the desktop (same resolution as repositories.forIssue).
      const repo = await resolveProjectRepository(issueCtx.projectId)
      if (!repo) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `No repository linked to this project — link one in workspace settings`,
        })
      }

      const result = await relayPostStart(config, {
        userId,
        deviceId: input.deviceId,
        issueId: input.issueId,
        model: input.model,
        effort: input.effort,
        ultracode: input.ultracode,
        planMode: input.planMode,
      })
      if (!result.ok) {
        if (result.status === 404) {
          // Device offline (or otherwise unroutable) — surface the relay reason.
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: result.reason,
          })
        }
        throw new TRPCError({
          code: `INTERNAL_SERVER_ERROR`,
          message: `Steer relay error (${result.status})`,
        })
      }
      return { ok: true as const }
    }),

  // Kill-switch: flip the synced row to ended (the desktop watches its own
  // coding_sessions row over Electric, so this aborts the run even if the
  // relay is unreachable) AND best-effort fan a kill through the relay so the
  // live terminal tears down immediately.
  killSession: authedProcedure
    .input(z.object({ codingSessionId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const session = await loadCodingSession(input.codingSessionId)
      const userId = ctx.session.user.id

      // Permission: the session owner, or a workspace owner.
      if (session.userId !== userId) {
        await assertWorkspaceMember(userId, session.workspaceId, [`owner`])
      }

      // Idempotent: killing an already-ended session leaves the row alone.
      let result: { session: typeof session; txid?: number }
      if (session.status === `ended`) {
        result = { session }
      } else {
        result = await ctx.db.transaction(async (tx) => {
          const txid = await generateTxId(tx)
          const [updated] = await tx
            .update(codingSessions)
            .set({ status: `ended`, endedAt: new Date() })
            .where(eq(codingSessions.id, input.codingSessionId))
            .returning()
          return { session: updated, txid }
        })
      }

      // Best-effort relay kill; swallow failure (relayPostKill never throws).
      const config = getSteerRelayConfig()
      if (config) await relayPostKill(config, input.codingSessionId)

      return result
    }),
})
