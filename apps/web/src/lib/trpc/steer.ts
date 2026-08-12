import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, eq, isNull } from "drizzle-orm"
import { contract } from "@exp/domain-contract"
import {
  MAX_ACTION_INPUTS,
  MAX_ACTION_INPUT_KEY,
  MAX_ACTION_INPUT_TEXT,
  type ActionInputDef,
} from "@exp/db-schema/domain"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import {
  actions,
  boards,
  codingSessions,
  devices as devicesTable,
  issues,
  repositories,
  teamMembers,
} from "@/db/schema"
import {
  assertTeamMember,
  getIssueTeamContext,
} from "@/lib/team-membership"
import {
  effectiveDefaultBranch,
  resolveBoardRepository,
} from "@/lib/trpc/repositories"
import {
  getSteerRelayConfig,
  mintSteerTicket,
  relayGetDevices,
  relayPostKill,
  relayPostStart,
  type SteerDevice,
  type SteerStartRepo,
} from "@/lib/steer"
import { resolveActionInputs } from "@/lib/action-inputs"
import {
  BUILTIN_CREATE_ACTION_ID,
  BUILTIN_FIX_CONFLICTS_ID,
  builtinActionName,
  builtinCreateAction,
  builtinFixConflictsAction,
  isBuiltinActionId,
} from "@/lib/builtin-actions"

// Remote start + live terminal steer (masterplan §3.5). The web app is the
// only place that holds STEER_RELAY_SECRET: it mints short-lived HS256 relay
// tickets after checking authorization (EXP-312: session tickets are
// OWNER-ONLY — a live session is visible and steerable only by the account
// that started it), and talks to the relay's secret-authed admin HTTP for
// device presence / remote start / kill.
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
  // Web/mobile live view of a session — owner-only (EXP-312).
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

      // Only the session owner's own desktop may publish its PTY — or, for a
      // shared-device run (EXP-432), the hosting daemon's account: the row is
      // requester-owned while the device owner's daemon runs the agent and
      // publishes its activity.
      if (input.kind === `publisher`) {
        if (session.userId !== userId && session.hostUserId !== userId) {
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

      // EXP-312: a live session is visible and steerable ONLY by its owner —
      // there is no view/steer distinction and no teammate access at all.
      // EXP-432 adds exactly one more principal: the hosting device owner may
      // watch what runs on their own hardware (it originates there anyway).
      if (session.userId !== userId && session.hostUserId !== userId) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only the session owner can open a live session`,
        })
      }
      // A requester steering a run on someone ELSE's shared device must still
      // be a member of the session's team — removal from the team would
      // otherwise leave them typing into the host's machine via re-minted
      // tickets for as long as the session lives.
      if (
        session.userId === userId &&
        session.hostUserId !== null &&
        session.hostUserId !== userId
      ) {
        await assertTeamMember(userId, session.teamId)
      }
      return mintSteerTicket(config, {
        kind: `viewer`,
        userId,
        teamId: session.teamId,
        sessionId: session.id,
      })
    }),

  // The caller's online desktops — powers the "Start on my desktop" picker.
  myDevices: authedProcedure.query(async ({ ctx }) => {
    const config = getSteerRelayConfig()
    if (!config) return { devices: [] as SteerDevice[] }
    return relayGetDevices(config, ctx.session.user.id)
  }),

  // Remote "Start on my desktop": route a start command to the chosen online
  // device's control socket via the relay. Accepts a single issueId
  // (wire-unchanged), issueIds (2..30 → a batch run on one shared branch), or
  // an actionId (EXP-253 — run a team action on the trunk clone / a scratch
  // dir); exactly one form. Everything is validated + resolved here — the
  // batch and action forms carry a server-resolved repo group because the
  // desktop syncs no repositories, and the relay stays a dumb pipe. The
  // optional launch options are the client's Start-coding dialog choices
  // (EXP-149) — validated against the domain-contract value sets here; absent
  // fields mean desktop settings defaults with plan mode OFF. Since EXP-257
  // action runs take the FULL option set (any installed agent + toggles) and
  // may carry `inputs` — the filled values for the action's input schema,
  // resolved + validated here before the frame rides the relay.
  startSession: authedProcedure
    .input(
      z
        .object({
          issueId: z.string().uuid().optional(),
          issueIds: z.array(z.string().uuid()).min(1).max(30).optional(),
          actionId: z
            .string()
            .uuid()
            .or(z.literal(BUILTIN_CREATE_ACTION_ID))
            .or(z.literal(BUILTIN_FIX_CONFLICTS_ID))
            .optional(),
          // Required iff actionId is the builtin (there is no DB row to
          // derive the team from); forbidden otherwise.
          teamId: z.string().uuid().optional(),
          // Raw filled input values, keyed by the schema's input keys
          // (repo/board values are picked ids). Validated against the
          // action's schema in the mutation.
          inputs: z
            .record(
              z.string().max(MAX_ACTION_INPUT_KEY),
              z.string().max(MAX_ACTION_INPUT_TEXT)
            )
            .refine((v) => Object.keys(v).length <= MAX_ACTION_INPUTS, {
              message: `Too many inputs`,
            })
            .optional(),
          deviceId: z.string().min(1).max(128),
          agent: z.enum(codingAgentValues).optional(),
          model: z.string().max(64).optional(),
          effort: z.string().max(32).optional(),
          ultracode: z.boolean().optional(),
          planMode: z.boolean().optional(),
          skipPermissions: z.boolean().optional(),
          // EXP-481: resume the issue's existing worktree/agent session.
          // Single-issue starts only; gated below on the device's persisted
          // `resume` cap.
          resume: z.boolean().optional(),
        })
        .refine(
          (value) =>
            [
              Boolean(value.issueId),
              Boolean(value.issueIds?.length),
              Boolean(value.actionId),
            ].filter(Boolean).length === 1,
          { message: `Exactly one of issueId/issueIds/actionId is required` }
        )
        .superRefine((value, ctx) => {
          const builtin =
            value.actionId !== undefined && isBuiltinActionId(value.actionId)
          if (builtin && !value.teamId) {
            ctx.addIssue({
              code: z.ZodIssueCode.custom,
              path: [`teamId`],
              message: `teamId is required for built-in actions`,
            })
          }
          if (!builtin && value.teamId) {
            ctx.addIssue({
              code: z.ZodIssueCode.custom,
              path: [`teamId`],
              message: `teamId rides built-in action starts only`,
            })
          }
          if (value.inputs && !value.actionId) {
            ctx.addIssue({
              code: z.ZodIssueCode.custom,
              path: [`inputs`],
              message: `inputs ride action starts only`,
            })
          }
          // EXP-481: a batch has no per-issue worktree to resume, and action
          // runs don't use issue worktrees at all. (A batch that collapses to
          // one id below still counts as the issueIds form — resume requires
          // the literal issueId field.)
          if (value.resume && !value.issueId) {
            ctx.addIssue({
              code: z.ZodIssueCode.custom,
              path: [`resume`],
              message: `resume applies to single-issue starts only`,
            })
          }
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
          if (agent !== `claude` && value.ultracode) {
            ctx.addIssue({
              code: z.ZodIssueCode.custom,
              message: `ultracode is a Claude-only option`,
            })
          }
          if (agent === `codex` && value.planMode) {
            ctx.addIssue({
              code: z.ZodIssueCode.custom,
              message: `planMode is a claude/pi-only option`,
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

      const fetchDevices = async (
        forUserId: string = userId
      ): Promise<SteerDevice[]> => {
        try {
          const { devices } = await relayGetDevices(config, forUserId)
          return devices
        } catch {
          throw new TRPCError({
            code: `BAD_GATEWAY`,
            message: `Couldn't reach the steer relay. Try again.`,
          })
        }
      }

      // EXP-432: resolve which account's presence bucket the target deviceId
      // lives in. The caller's own presence wins (today's path — the start
      // stays wire-identical); otherwise a registered server device SHARED
      // with the subject's team resolves to its owner's bucket — the caller's
      // membership in that team is already asserted by the subject checks
      // above each call site, and the daemon re-verifies the share when it
      // echoes `startedBy` into codingSessions.start. Anything else keeps the
      // legacy lenient shape (device undefined; the relay answers
      // device_offline).
      const resolveTargetDevice = async (
        teamId: string
      ): Promise<{
        ownerId: string
        device: SteerDevice | undefined
        shared: boolean
      }> => {
        const own = await fetchDevices()
        const ownDevice = own.find((d) => d.deviceId === input.deviceId)
        if (ownDevice) {
          return { ownerId: userId, device: ownDevice, shared: false }
        }
        const { db } = await import(`@/db/connection`)
        // The team_members join mirrors devices.list: a lingering share whose
        // owner left the team must not route starts to their machine (the
        // daemon's own codingSessions.start would fail on the owner's gone
        // membership anyway — this just refuses before waking the box).
        const [row] = await db
          .select({ userId: devicesTable.userId })
          .from(devicesTable)
          .innerJoin(
            teamMembers,
            and(
              eq(teamMembers.userId, devicesTable.userId),
              eq(teamMembers.teamId, teamId)
            )
          )
          .where(
            and(
              eq(devicesTable.deviceId, input.deviceId),
              eq(devicesTable.sharedTeamId, teamId),
              eq(devicesTable.kind, `server`)
            )
          )
          .limit(1)
        if (!row || row.userId === userId) {
          return { ownerId: userId, device: undefined, shared: false }
        }
        const ownerDevices = await fetchDevices(row.userId)
        return {
          ownerId: row.userId,
          device: ownerDevices.find((d) => d.deviceId === input.deviceId),
          shared: true,
        }
      }

      // Action run (EXP-253/EXP-257): resolve the action (a DB row, or the
      // virtual builtin composed from constants) → team + optional repo
      // group + resolved input values, then require the target device to
      // advertise the `actions` capability — STRICT, unlike the lenient
      // agents fallback below: an old desktop can run claude but has no
      // action launch path at all. Builtin or inputs-carrying starts
      // additionally require `action-inputs` (an old desktop would silently
      // drop the inputs field and run a valueless prompt).
      if (input.actionId) {
        const { db } = await import(`@/db/connection`)

        let action: {
          id: string
          teamId: string
          repositoryId: string | null
          name: string
        }
        let inputDefs: ActionInputDef[]
        const builtin = isBuiltinActionId(input.actionId)
        if (builtin) {
          await assertTeamMember(userId, input.teamId!)
          const virtual =
            input.actionId === BUILTIN_FIX_CONFLICTS_ID
              ? builtinFixConflictsAction(input.teamId!)
              : builtinCreateAction(input.teamId!)
          action = {
            id: virtual.id,
            teamId: virtual.teamId,
            repositoryId: null,
            name: builtinActionName(virtual.id),
          }
          inputDefs = virtual.inputs
        } else {
          const [row] = await db
            .select({
              id: actions.id,
              teamId: actions.teamId,
              repositoryId: actions.repositoryId,
              name: actions.name,
              inputs: actions.inputs,
            })
            .from(actions)
            .where(eq(actions.id, input.actionId))
            .limit(1)
          if (!row) {
            throw new TRPCError({
              code: `NOT_FOUND`,
              message: `Action not found`,
            })
          }
          await assertTeamMember(userId, row.teamId)
          action = row
          inputDefs = row.inputs
        }

        // Validate + resolve the filled input values against the schema —
        // display names ride the frame so the desktop needs no lookups.
        const resolved = await resolveActionInputs(
          inputDefs,
          input.inputs ?? {},
          action.teamId,
          {
            repo: async (id, teamId) => {
              const [row] = await db
                .select({
                  teamId: repositories.teamId,
                  fullName: repositories.fullName,
                })
                .from(repositories)
                .where(eq(repositories.id, id))
                .limit(1)
              return row && row.teamId === teamId
                ? { fullName: row.fullName }
                : null
            },
            board: async (id, teamId) => {
              const [row] = await db
                .select({ teamId: boards.teamId, name: boards.name })
                .from(boards)
                .where(and(eq(boards.id, id), isNull(boards.deletedAt)))
                .limit(1)
              return row && row.teamId === teamId ? { name: row.name } : null
            },
            pr: async (issueId, teamId) => {
              const [row] = await db
                .select({
                  teamId: issues.teamId,
                  identifier: issues.identifier,
                  prNumber: issues.prNumber,
                  prState: issues.prState,
                })
                .from(issues)
                .where(eq(issues.id, issueId))
                .limit(1)
              return row && row.teamId === teamId && row.prState === `open`
                ? { identifier: row.identifier, prNumber: row.prNumber }
                : null
            },
          }
        )
        if (!resolved.ok) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: resolved.message,
          })
        }

        // Strip to the relay-safe repo group (never installationId). A row
        // gone can't happen (FK SET NULL nulls repositoryId); an archived or
        // inaccessible repo passes through — the desktop clone fails visibly.
        let repo: SteerStartRepo | undefined
        if (input.actionId === BUILTIN_FIX_CONFLICTS_ID) {
          // The fix-conflicts builtin's repo is the picked PR's — derived
          // from its issue's board, exactly the repo the PR branch lives in.
          const prIssueId = resolved.inputs.find(
            (value) => value.key === `pr`
          )?.value
          const [issueRow] = prIssueId
            ? await db
                .select({ boardId: issues.boardId })
                .from(issues)
                .where(eq(issues.id, prIssueId))
                .limit(1)
            : []
          const [repoRow] = issueRow
            ? await db
                .select({
                  id: repositories.id,
                  fullName: repositories.fullName,
                  defaultBranch: repositories.defaultBranch,
                  defaultBranchOverride: repositories.defaultBranchOverride,
                })
                .from(boards)
                .innerJoin(
                  repositories,
                  eq(repositories.id, boards.repositoryId)
                )
                .where(eq(boards.id, issueRow.boardId))
                .limit(1)
            : []
          if (!repoRow) {
            throw new TRPCError({
              code: `PRECONDITION_FAILED`,
              message: `The pull request's board has no linked repository`,
            })
          }
          repo = {
            repositoryId: repoRow.id,
            fullName: repoRow.fullName,
            defaultBranch: effectiveDefaultBranch(repoRow),
          }
        } else if (action.repositoryId) {
          const [row] = await db
            .select({
              id: repositories.id,
              fullName: repositories.fullName,
              defaultBranch: repositories.defaultBranch,
              defaultBranchOverride: repositories.defaultBranchOverride,
            })
            .from(repositories)
            .where(eq(repositories.id, action.repositoryId))
            .limit(1)
          if (row) {
            repo = {
              repositoryId: row.id,
              fullName: row.fullName,
              defaultBranch: effectiveDefaultBranch(row),
            }
          }
        }

        const { ownerId, device, shared } = await resolveTargetDevice(
          action.teamId
        )
        const caps = device?.caps ?? []
        if (!device || !caps.includes(`actions`)) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `That desktop app can't run actions yet. Update it.`,
          })
        }
        if (
          (builtin || resolved.inputs.length > 0) &&
          !caps.includes(`action-inputs`)
        ) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `That desktop app can't run action inputs yet. Update it.`,
          })
        }
        // The fix-conflicts builtin (EXP-259) needs its own launch path on
        // the device — a pre-EXP-259 desktop advertises `actions` +
        // `action-inputs` but would treat the id as a real action and fail
        // its fetch.
        if (
          input.actionId === BUILTIN_FIX_CONFLICTS_ID &&
          !caps.includes(`fix-conflicts`)
        ) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `That desktop app can't fix merge conflicts yet. Update it.`,
          })
        }
        // EXP-257: actions run on any agent the device advertised, same
        // lenient fallback as the issue branch (nothing advertised ⇒ claude).
        const actionAgent = input.agent ?? `claude`
        // ...but only on a desktop that actually LAUNCHES the chosen agent. A
        // pre-EXP-257 desktop advertises `actions` and its full agent list,
        // yet its action runner clamps the agent to claude while still
        // honoring the model string — a codex/pi start would silently launch
        // claude with a foreign model. `action-inputs` is the EXP-257 marker
        // cap, so it is the reliable "this desktop honors the agent" test.
        // (The new toggles are NOT gated: an old desktop ignores them and
        // falls back to its own device settings, exactly as it did before
        // EXP-257 offered them — degraded, not a regression.)
        if (actionAgent !== `claude` && !caps.includes(`action-inputs`)) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `That desktop app can only run actions on claude yet. Update it.`,
          })
        }
        const actionDeviceAgents =
          device.agents && device.agents.length > 0
            ? device.agents
            : [`claude`]
        if (!actionDeviceAgents.includes(actionAgent)) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `${actionAgent} is not installed on that device`,
          })
        }

        const result = await relayPostStart(config, {
          userId: ownerId,
          deviceId: input.deviceId,
          ...(shared ? { startedBy: userId } : {}),
          actionId: action.id,
          actionName: action.name,
          teamId: action.teamId,
          ...(repo ? { repo } : {}),
          ...(resolved.inputs.length > 0 ? { inputs: resolved.inputs } : {}),
          agent: input.agent,
          model: input.model,
          effort: input.effort,
          ultracode: input.ultracode,
          planMode: input.planMode,
          skipPermissions: input.skipPermissions,
        })
        if (!result.ok) {
          if (result.status === 404) {
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
      }

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
            message: `No repository linked to this board. Link one in team settings.`,
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
      // nothing ⇒ claude-only, exactly what it can do). An EXPLICITLY empty
      // list (EXP-409) means nothing is runnable — the claude fallback only
      // covers the absent-advertisement legacy case. A signed-out agent gets
      // the sign-in message, not "not installed".
      const agent = input.agent ?? `claude`
      const { ownerId, device, shared } = await resolveTargetDevice(teamId)
      const deviceAgentIds = device?.agents ?? [`claude`]
      if (device && !deviceAgentIds.includes(agent)) {
        const signedOut = (device.unauthedAgents ?? []).includes(agent)
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: signedOut
            ? `${agent} is installed on that device but not signed in — sign in on the machine first`
            : `${agent} is not installed on that device`,
        })
      }

      // EXP-481: resume is gated STRICTLY on the PERSISTED row's `resume` cap
      // (like `actions`, unlike the lenient agents fallback) — an old build
      // would silently drop the flag and start fresh, which reads as data
      // loss to someone expecting their session back. Persisted caps are as
      // accurate as live ones here: caps are immutable per build and
      // registered at startup; an unregistered legacy desktop therefore
      // refuses, correctly.
      if (input.resume) {
        const { db } = await import(`@/db/connection`)
        const [target] = await db
          .select({ caps: devicesTable.caps })
          .from(devicesTable)
          .where(
            and(
              eq(devicesTable.userId, ownerId),
              eq(devicesTable.deviceId, input.deviceId)
            )
          )
          .limit(1)
        if (!target?.caps.includes(`resume`)) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `That device can't resume sessions yet. Update it.`,
          })
        }
      }

      const options = {
        agent: input.agent,
        model: input.model,
        effort: input.effort,
        ultracode: input.ultracode,
        planMode: input.planMode,
        skipPermissions: input.skipPermissions,
        resume: input.resume,
      }
      const result =
        ids.length === 1
          ? await relayPostStart(config, {
              userId: ownerId,
              deviceId: input.deviceId,
              ...(shared ? { startedBy: userId } : {}),
              issueId: ids[0]!,
              ...options,
            })
          : await relayPostStart(config, {
              userId: ownerId,
              deviceId: input.deviceId,
              ...(shared ? { startedBy: userId } : {}),
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

      // EXP-312: owner-only, like every other live-session control — plus the
      // hosting device owner (EXP-432): they can always kill what runs on
      // their own machine.
      if (session.userId !== userId && session.hostUserId !== userId) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only the session owner can kill it`,
        })
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
