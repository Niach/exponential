import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { contract } from "@exp/domain-contract"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { codingSessions } from "@/db/schema"
import {
  assertTeamMember,
  getIssueTeamContext,
} from "@/lib/team-membership"
import { resolveBoardRepository } from "@/lib/trpc/repositories"
import {
  getSteerRelayConfig,
  mintSteerTicket,
  relayGetDevices,
  relayPostKill,
  relayPostStart,
  type SteerDevice,
  type SteerStartRepo,
} from "@/lib/steer"

// Remote start + live terminal steer (masterplan §3.5). The web app is the
// only place that holds STEER_RELAY_SECRET: it mints short-lived HS256 relay
// tickets after checking the caller's team permission, and talks to the
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

// The Start-coding dialog value sets (EXP-149/EXP-201). Blank model/effort is
// the explicit "CLI default" (omit the flag) — a per-client extra row, not a
// contract value; claude's model stays explicit-always (no blank).
const codingAgentValues = contract.codingAgent.values as [string, ...string[]]
const agentModelValues: Record<string, readonly string[]> = {
  claude: contract.codingModel.values,
  codex: [``, ...contract.codexModel.values],
  pi: [``, ...contract.piModel.values],
}
const agentEffortValues: Record<string, readonly string[]> = {
  claude: [``, ...contract.codingEffort.values],
  codex: [``, ...contract.codexEffort.values],
  pi: [``, ...contract.piThinking.values],
}

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
          teamId: session.teamId,
          sessionId: session.id,
        })
      }

      // Viewers must be members of the session's team; team owners
      // may steer, and so may the session's own starter — plain members watch
      // (viewerPermFor).
      const member = await assertTeamMember(userId, session.teamId)
      return mintSteerTicket(config, {
        kind: `viewer`,
        userId,
        teamId: session.teamId,
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
  // device's control socket via the relay. Accepts EITHER a single issueId
  // (wire-unchanged) or issueIds (2..30 → a batch run on one shared branch);
  // exactly one form. Everything is validated + resolved here — the batch form
  // carries a server-resolved repo group because the desktop syncs no
  // repositories, and the relay stays a dumb pipe. The optional launch options
  // are the client's Start-coding dialog choices (EXP-149) — validated against
  // the domain-contract value sets here; absent fields mean desktop settings
  // defaults with plan mode OFF.
  startSession: authedProcedure
    .input(
      z
        .object({
          issueId: z.string().uuid().optional(),
          issueIds: z.array(z.string().uuid()).min(1).max(30).optional(),
          deviceId: z.string().min(1).max(128),
          agent: z.enum(codingAgentValues).optional(),
          model: z.string().max(64).optional(),
          effort: z.string().max(32).optional(),
          ultracode: z.boolean().optional(),
          planMode: z.boolean().optional(),
          skipPermissions: z.boolean().optional(),
        })
        .refine(
          (value) => Boolean(value.issueId) !== Boolean(value.issueIds?.length),
          { message: `Exactly one of issueId/issueIds is required` }
        )
        .superRefine((value, ctx) => {
          // Per-agent vocabulary (EXP-201): model/effort must come from the
          // (agent ?? claude) contract lists, and the claude-only toggles may
          // not ride a codex/pi start (pi additionally has no permission
          // system to skip).
          const agent = value.agent ?? `claude`
          if (
            value.model !== undefined &&
            !agentModelValues[agent]!.includes(value.model)
          ) {
            ctx.addIssue({
              code: z.ZodIssueCode.custom,
              path: [`model`],
              message: `Unknown ${agent} model`,
            })
          }
          if (
            value.effort !== undefined &&
            !agentEffortValues[agent]!.includes(value.effort)
          ) {
            ctx.addIssue({
              code: z.ZodIssueCode.custom,
              path: [`effort`],
              message: `Unknown ${agent} effort`,
            })
          }
          if (agent !== `claude` && (value.ultracode || value.planMode)) {
            ctx.addIssue({
              code: z.ZodIssueCode.custom,
              message: `ultracode/planMode are Claude-only options`,
            })
          }
          if (agent === `pi` && value.skipPermissions) {
            ctx.addIssue({
              code: z.ZodIssueCode.custom,
              path: [`skipPermissions`],
              message: `pi has no permission system to skip`,
            })
          }
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

      // One issue (wire-unchanged) or a batch (2..30). Collapse duplicates so a
      // caller can't inflate the batch or repeat past the length cap; a batch
      // that collapses to a single id falls back to the legacy single wire.
      const ids = [...new Set(input.issueIds ?? [input.issueId!])]

      // Every issue must live in ONE team — the membership check is
      // team-scoped and a batch pushes a single shared branch.
      const contexts = await Promise.all(
        ids.map((id) => getIssueTeamContext(id))
      )
      const teamIds = new Set(contexts.map((c) => c.teamId))
      if (teamIds.size > 1) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `All issues in a batch must be in one team`,
        })
      }
      const teamId = contexts[0]!.teamId
      await assertTeamMember(userId, teamId)

      // The launcher can't do anything without a linked repo, and a batch must
      // land in ONE repo (mirrors the MCP pr_open loop) — resolve per distinct
      // board and fail before waking the desktop.
      const boardIds = [...new Set(contexts.map((c) => c.boardId))]
      let repo: SteerStartRepo | null = null
      for (const boardId of boardIds) {
        const resolved = await resolveBoardRepository(boardId)
        if (!resolved) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `No repository linked to this board — link one in team settings`,
          })
        }
        if (repo && repo.repositoryId !== resolved.repositoryId) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `All issues in a batch must share one repository (${repo.fullName} vs ${resolved.fullName})`,
          })
        }
        // Strip installationId — the repo group never rides the relay.
        repo = {
          repositoryId: resolved.repositoryId,
          fullName: resolved.fullName,
          defaultBranch: resolved.defaultBranch,
        }
      }

      // EXP-201: the target device advertised which agent CLIs it can run —
      // refuse a start naming one it didn't (an old desktop advertises
      // nothing ⇒ claude-only, exactly what it can do).
      const agent = input.agent ?? `claude`
      const { devices } = await relayGetDevices(config, userId)
      const device = devices.find((d) => d.deviceId === input.deviceId)
      const deviceAgentIds =
        device?.agents && device.agents.length > 0 ? device.agents : [`claude`]
      if (device && !deviceAgentIds.includes(agent)) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `${agent} is not installed on that device`,
        })
      }

      const options = {
        agent: input.agent,
        model: input.model,
        effort: input.effort,
        ultracode: input.ultracode,
        planMode: input.planMode,
        skipPermissions: input.skipPermissions,
      }
      const result =
        ids.length === 1
          ? await relayPostStart(config, {
              userId,
              deviceId: input.deviceId,
              issueId: ids[0]!,
              ...options,
            })
          : await relayPostStart(config, {
              userId,
              deviceId: input.deviceId,
              issueIds: ids,
              teamId,
              repo: repo!,
              ...options,
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

      // Permission: the session owner, or a team owner.
      if (session.userId !== userId) {
        await assertTeamMember(userId, session.teamId, [`owner`])
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
