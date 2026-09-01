import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, desc, eq, gte, inArray, or } from "drizzle-orm"
import { contract } from "@exp/domain-contract"
import {
  CODING_SESSION_STALE_MS,
  startedReasonValues,
} from "@exp/db-schema/domain"
import { router, authedProcedure, type Context } from "@/lib/trpc"
import { notifyParentOfChildEnd } from "@/lib/steer-child-messages"
import { actions, automations, codingSessions, devices, issues } from "@/db/schema"
import {
  assertTeamMember,
  getIssueTeamContext,
} from "@/lib/team-membership"
import {
  BUILTIN_CHAT_ID,
  BUILTIN_CREATE_ACTION_ID,
  BUILTIN_FIX_CONFLICTS_ID,
  builtinActionName,
  isBuiltinActionId,
} from "@/lib/builtin-actions"

// Built-in action runs (EXP-257/EXP-259) have no DB row to FK — their session
// rows are batch-shaped (actionId NULL) with the server-constant name
// snapshot, which also makes clients' actionName-based run watching work
// uniformly.
const codingAgentValues = contract.codingAgent.values as [string, ...string[]]

const actionIdInput = z
  .string()
  .uuid()
  .or(z.literal(BUILTIN_CREATE_ACTION_ID))
  .or(z.literal(BUILTIN_FIX_CONFLICTS_ID))
  .or(z.literal(BUILTIN_CHAT_ID))

// EXP-432: a remote start on a teammate's SHARED server device is attributed
// to the requester — `startedBy` rides the relay frame and the daemon echoes
// it here as `startedById` (plus its own `deviceId` so the share can be
// verified). Trust model: the caller (the daemon owner) may attribute a
// session to a teammate ONLY when their own device row says they shared that
// device with the session's team and the teammate is still a member — sharing
// IS the consent to host teammate-owned runs. Everything else (absent field,
// self-attribution) is the pre-EXP-432 path: the row belongs to the caller.
// EXP-445 on the residual forgery: naming a `startedById` takes the device
// owner's OWN credentials (the daemon is the authed caller here), can only
// name a member of a team that owner explicitly shared the box with, and
// grants that member no privilege they lack — it is attribution noise, not
// escalation. Closing it properly means relay-signed start tokens carrying the
// requester, which is out of scope here.
async function resolveStartAttribution(
  db: Context[`db`],
  callerId: string,
  input: { startedById?: string; deviceId?: string },
  teamId: string
): Promise<{ userId: string; hostUserId: string | null }> {
  if (!input.startedById || input.startedById === callerId) {
    return { userId: callerId, hostUserId: null }
  }
  if (!input.deviceId) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `startedById requires the sharing device's deviceId`,
    })
  }
  const [device] = await db
    .select({ kind: devices.kind, sharedTeamId: devices.sharedTeamId })
    .from(devices)
    .where(
      and(eq(devices.userId, callerId), eq(devices.deviceId, input.deviceId))
    )
    .limit(1)
  if (!device || device.kind !== `server` || device.sharedTeamId !== teamId) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `This device is not shared with the session's team`,
    })
  }
  await assertTeamMember(input.startedById, teamId)
  return { userId: input.startedById, hostUserId: callerId }
}

// EXP-549: the session's host-device stamp. The device sends its steer
// `deviceId`; the label snapshot comes from the registry row's `label` (the
// user's RENAME, `devices.rename`) whenever the caller — the hosting account
// in both self-hosted and shared-device runs — owns a row for that deviceId.
// Clients prefer the live devices row via `device_id`; the snapshot renders
// historical rows. EXP-560 retired the label-only pre-EXP-549 wire.
// `deviceLabel` on `start` is a FALLBACK, not compat: `devices.register` is
// fire-and-forget on the desktop and the CLI daemon, so a start fired
// immediately after launch can beat the registration and find no row at all —
// without the sent hostname that run's snapshot would be NULL forever
// (heartbeats only refresh rows the registry can answer for). Heartbeat never
// takes one: by then the registry has answered, and a label that could ride
// alone would let a client overwrite the user's rename.
async function resolveSessionDevice(
  db: Context[`db`],
  callerId: string,
  input: { deviceId?: string; deviceLabel?: string }
): Promise<{ deviceId: string | null; deviceLabel: string | null }> {
  if (!input.deviceId) {
    return { deviceId: null, deviceLabel: null }
  }
  const [device] = await db
    .select({ label: devices.label })
    .from(devices)
    .where(
      and(eq(devices.userId, callerId), eq(devices.deviceId, input.deviceId))
    )
    .limit(1)
  return {
    deviceId: input.deviceId,
    deviceLabel: device?.label ?? input.deviceLabel ?? null,
  }
}

// EXP-637's resume link, hardened (EXP-639). `resumed_from_id` is a real FK
// and history ONLY — never authorization — but the run it names may be gone:
// the 2h idle sweep (lib/coding-session-sweep.ts) DELETES stale running rows,
// so a desktop run-registry record easily outlives its session and the insert
// would fail with a raw 23503 (a 500 on the user's Resume click). Resolve it
// first and degrade to NULL when the row no longer exists; scope is
// deliberately not checked (the link is provenance, and the row is only ever
// read back as the caller's own history).
async function resolveResumedFromId(
  db: Context[`db`],
  resumedFromId: string | undefined
): Promise<string | null> {
  if (!resumedFromId) return null
  const [row] = await db
    .select({ id: codingSessions.id })
    .from(codingSessions)
    .where(eq(codingSessions.id, resumedFromId))
    .limit(1)
  return row?.id ?? null
}

// The desktop launcher's live "coding now" record (§4a step 7). One row per
// interactive session; synced to every client as an Electric shape.
// Three subjects: issue-scoped (issueId), batch-scoped (teamId — the
// desktop multi-issue batch orchestrator; issue_id/board_id stay NULL, the
// populate triggers no-op on NULL issue_id), or action-scoped (actionId —
// EXP-253: batch-shaped plus action_id + the action_name display snapshot;
// actions are server-only so clients label rows off the snapshot).
// Exactly one of the three.
// No generateTxId — native callers don't need the Electric tx-wait, and the
// row's own synced propagation carries the badge.
// EXP-583: the automation a device claims fired this run must exist and
// target this very action — a stale/foreign id degrades to NULL (history
// only; never a reason to refuse the start).
async function resolveAutomationId(
  db: Context[`db`],
  automationId: string,
  actionId: string
): Promise<string | null> {
  const [row] = await db
    .select({ id: automations.id })
    .from(automations)
    .where(and(eq(automations.id, automationId), eq(automations.actionId, actionId)))
    .limit(1)
  return row?.id ?? null
}

export const codingSessionsRouter = router({
  // Own-row status probe (EXP-403): the headless CLI daemon has no Electric
  // sync, so it polls this for the →ended kill-switch edge the desktop's
  // kill-watch reads off the synced collection. Owner-OR-HOST by the where
  // clause (EXP-432: a shared-device session is requester-owned while the
  // polling daemon is only its host — owner-only scoping would return null
  // and a remote kill would never reach the agent); a swept (deleted) row
  // returns { session: null }, which — like the desktop's vanished-row rule —
  // deliberately does NOT read as a kill.
  get: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const [session] = await ctx.db
        .select()
        .from(codingSessions)
        .where(
          and(
            eq(codingSessions.id, input.id),
            or(
              eq(codingSessions.userId, ctx.session.user.id),
              eq(codingSessions.hostUserId, ctx.session.user.id)
            )
          )
        )
        .limit(1)
      return { session: session ?? null }
    }),

  // EXP-403: the CLI daemon's REV2-24 one-session-per-issue probe — the
  // desktop reads its synced coding_sessions collection for this; the
  // headless daemon has no sync and asks the server instead. "Live" mirrors
  // the client predicate: status still alive (in_review sessions stay
  // steerable) AND updated_at within the staleness window.
  // Member-scoped via the issue's team.
  liveForIssue: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const issueCtx = await getIssueTeamContext(input.issueId)
      await assertTeamMember(ctx.session.user.id, issueCtx.teamId)
      const [session] = await ctx.db
        .select({
          id: codingSessions.id,
          deviceLabel: codingSessions.deviceLabel,
          userId: codingSessions.userId,
        })
        .from(codingSessions)
        .where(
          and(
            eq(codingSessions.issueId, input.issueId),
            inArray(codingSessions.status, [`running`, `in_review`]),
            gte(
              codingSessions.updatedAt,
              new Date(Date.now() - CODING_SESSION_STALE_MS)
            )
          )
        )
        .orderBy(desc(codingSessions.updatedAt))
        .limit(1)
      return { session: session ?? null }
    }),

  start: authedProcedure
    .input(
      z
        .object({
          issueId: z.string().uuid().optional(),
          teamId: z.string().uuid().optional(),
          actionId: actionIdInput.optional(),
          // Label fallback for a start that outran `devices.register` — see
          // resolveSessionDevice. Never used when the registry has a row.
          deviceLabel: z.string().max(255).optional(),
          // EXP-432 shared-device attribution (see resolveStartAttribution).
          startedById: z.string().min(1).max(128).optional(),
          deviceId: z.string().min(1).max(128).optional(),
          // EXP-530: set by a device's automation host when an automation
          // fires. NULL/absent = a person started the run. EXP-583: the
          // firing automation's row id rides along for per-automation history.
          // EXP-679: `agent` instead means another coding session asked for
          // this run (via `exponential_sessions_start` → the relay frame) —
          // equally unattended, and unlike schedule/event it rides EVERY
          // subject, with no automation row behind it.
          startedReason: z.enum(startedReasonValues).optional(),
          automationId: z.string().uuid().optional(),
          // EXP-637: every repo-backed action/chat run now gets its own
          // worktree + branch (`exp/<slug>-<id8>`), so the row records the
          // branch the same way a batch row does once its PR opens. Issue
          // rows never carry it — the issue itself owns `exp/<IDENTIFIER>`.
          branch: z.string().max(255).optional(),
          // EXP-637: the ended run this one continues (desktop Resume or
          // steer.startSession({ resumeSessionId })). History only.
          resumedFromId: z.string().uuid().optional(),
          // EXP-484: the agent CLI running the session, recorded so every
          // client can name it (and pair the run with the host device's usage
          // windows). Absent on rows from clients that predate it.
          agent: z.enum(codingAgentValues).optional(),
        })
        .refine((value) => !(value.branch && value.issueId), {
          message: `branch excludes issueId — an issue session's branch lives on the issue`,
        })
        .refine(
          (value) => {
            // Built-in actions have no DB row to derive the team from — the
            // builtin literal REQUIRES teamId (and still excludes issueId).
            if (value.actionId && isBuiltinActionId(value.actionId)) {
              return Boolean(value.teamId) && !value.issueId
            }
            return (
              [value.issueId, value.teamId, value.actionId].filter(Boolean)
                .length === 1
            )
          },
          {
            message: `Exactly one of issueId/teamId/actionId is required`,
          }
        )
        .refine(
          (value) =>
            !value.startedReason ||
            // EXP-679: an agent-started run has no automation behind it and
            // no subject restriction — issue, batch, action, builtin and
            // resume all qualify. Only schedule/event still need a real
            // action row (an automation targets one).
            value.startedReason === `agent` ||
            (Boolean(value.actionId) && !isBuiltinActionId(value.actionId!)),
          {
            message: `startedReason requires a real actionId — only automations automate starts`,
          }
        )
        .refine(
          (value) =>
            !value.automationId ||
            (Boolean(value.startedReason) && value.startedReason !== `agent`),
          {
            // EXP-679: automationId belongs to schedule/event alone — an
            // agent-started run is nobody's automation history.
            message: `automationId requires startedReason`,
          }
        )
    )
    .mutation(async ({ ctx, input }) => {
      // A vanished predecessor (swept while the user was away) must never
      // turn Resume into a 500 — see resolveResumedFromId.
      const resumedFromId = await resolveResumedFromId(
        ctx.db,
        input.resumedFromId
      )

      if (input.actionId && isBuiltinActionId(input.actionId)) {
        await assertTeamMember(ctx.session.user.id, input.teamId!)
        const attribution = await resolveStartAttribution(
          ctx.db,
          ctx.session.user.id,
          input,
          input.teamId!
        )
        const device = await resolveSessionDevice(
          ctx.db,
          ctx.session.user.id,
          input
        )

        const [session] = await ctx.db
          .insert(codingSessions)
          .values({
            // Batch-shaped: actionId NULL (nothing to FK), the constant name
            // labels the run on every client.
            teamId: input.teamId!,
            actionId: null,
            actionName: builtinActionName(input.actionId),
            // EXP-679: only `agent` reaches a builtin (the refine keeps
            // schedule/event on real action rows).
            startedReason: input.startedReason ?? null,
            userId: attribution.userId,
            hostUserId: attribution.hostUserId,
            ...device,
            agent: input.agent ?? null,
            branch: input.branch ?? null,
            resumedFromId,
            status: `running`,
          })
          .returning()

        return { session }
      }

      if (input.actionId) {
        // Action run: every member may run a team action (running is a
        // member affordance; only WRITES are owner-gated). The name is
        // snapshotted server-side so the row outlives the action row.
        const [action] = await ctx.db
          .select({
            id: actions.id,
            teamId: actions.teamId,
            name: actions.name,
          })
          .from(actions)
          .where(eq(actions.id, input.actionId))
          .limit(1)
        if (!action) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Action not found`,
          })
        }
        await assertTeamMember(ctx.session.user.id, action.teamId)
        const attribution = await resolveStartAttribution(
          ctx.db,
          ctx.session.user.id,
          input,
          action.teamId
        )
        const device = await resolveSessionDevice(
          ctx.db,
          ctx.session.user.id,
          input
        )

        const [session] = await ctx.db
          .insert(codingSessions)
          .values({
            // Batch-shaped: no issue/board — team_id written directly.
            teamId: action.teamId,
            actionId: action.id,
            actionName: action.name,
            startedReason: input.startedReason ?? null,
            automationId: input.automationId
              ? await resolveAutomationId(ctx.db, input.automationId, action.id)
              : null,
            userId: attribution.userId,
            hostUserId: attribution.hostUserId,
            ...device,
            agent: input.agent ?? null,
            branch: input.branch ?? null,
            resumedFromId,
            status: `running`,
          })
          .returning()

        return { session }
      }

      if (input.issueId) {
        const issueCtx = await getIssueTeamContext(input.issueId)
        await assertTeamMember(ctx.session.user.id, issueCtx.teamId)
        const attribution = await resolveStartAttribution(
          ctx.db,
          ctx.session.user.id,
          input,
          issueCtx.teamId
        )
        const device = await resolveSessionDevice(
          ctx.db,
          ctx.session.user.id,
          input
        )

        const [session] = await ctx.db
          .insert(codingSessions)
          .values({
            issueId: input.issueId,
            // Set explicitly (also trigger-denormalized) so the row is valid even
            // if the populate_* triggers aren't applied.
            teamId: issueCtx.teamId,
            boardId: issueCtx.boardId,
            // EXP-679: an issue run can be agent-started (only `agent`
            // reaches here — schedule/event need a real action row).
            startedReason: input.startedReason ?? null,
            userId: attribution.userId,
            hostUserId: attribution.hostUserId,
            ...device,
            agent: input.agent ?? null,
            resumedFromId,
            status: `running`,
          })
          .returning()

        return { session }
      }

      await assertTeamMember(ctx.session.user.id, input.teamId!)
      const attribution = await resolveStartAttribution(
        ctx.db,
        ctx.session.user.id,
        input,
        input.teamId!
      )
      const device = await resolveSessionDevice(
        ctx.db,
        ctx.session.user.id,
        input
      )

      const [session] = await ctx.db
        .insert(codingSessions)
        .values({
          // Batch run: no issue to denormalize from — team_id written
          // directly; board_id stays NULL, a batch run spans boards and
          // must never surface through the anonymous board-scoped clause.
          teamId: input.teamId!,
          // EXP-679: a batch run can be agent-started (only `agent` reaches
          // here — schedule/event need a real action row).
          startedReason: input.startedReason ?? null,
          userId: attribution.userId,
          hostUserId: attribution.hostUserId,
          ...device,
          agent: input.agent ?? null,
          branch: input.branch ?? null,
          resumedFromId,
          status: `running`,
        })
        .returning()

      return { session }
    }),

  // Liveness ping from the desktop while the claude child is alive. The
  // server-side staleness sweep (lib/coding-session-sweep.ts) treats a
  // `running` row whose updated_at stopped advancing as a crashed desktop
  // and DELETES it — deliberately never flips it to `ended`, because that
  // transition is the desktop's remote-kill signal (a vanished row does not
  // fire the kill-switch), so deletion can never kill a live child.
  // A ping that finds its row GONE therefore means "swept while actually
  // alive" — a laptop suspend longer than CODING_SESSION_STALE_HOURS is the
  // routine case (EXP-105) — so when the client supplies the row's original
  // start scope, the row is re-created under the SAME id (fresh startedAt —
  // the original is lost with the row), restoring badge + steerability
  // within one heartbeat interval (an issue-scoped re-create derives
  // running/in_review from the issue's own status so a post-PR session
  // resurfaces with the right badge). An EXISTING `ended` row is NEVER
  // resurrected: `ended` is an explicit end/kill and must stay final.
  // `in_review` rows (PR open, terminal still alive — EXP-194) heartbeat
  // like running ones, but the ping only ever advances updated_at — it can
  // never downgrade in_review back to running.
  // Fire-and-forget on the client: failures are reported, never thrown.
  heartbeat: authedProcedure
    .input(
      z
        .object({
          id: z.string().uuid(),
          // The row's original start scope — enables re-create-on-missing.
          issueId: z.string().uuid().optional(),
          teamId: z.string().uuid().optional(),
          // Action scope (EXP-253) rides WITH teamId so a deleted action
          // still lets the row resurrect batch-shaped; actionName is the
          // client-held snapshot (the action may be gone by resurrect time).
          actionId: actionIdInput.optional(),
          actionName: z.string().max(255).optional(),
          // EXP-432: shared-device runs echo their attribution so a swept row
          // resurrects requester-owned instead of silently flipping to the
          // host (which would break the requester's steering mid-run).
          startedById: z.string().min(1).max(128).optional(),
          deviceId: z.string().min(1).max(128).optional(),
          // EXP-530: automated runs echo their reason so a swept row
          // resurrects inside the Automations run history, not as a
          // hand-started session. EXP-679: `agent` echoes on EVERY subject
          // (it needs no action row), so an agent-started run stays
          // unattended across a sweep.
          startedReason: z.enum(startedReasonValues).optional(),
          automationId: z.string().uuid().optional(),
          // EXP-637: run worktrees echo their branch so a swept row
          // resurrects tied to the same branch (the desktop's own-branch
          // guards and the unlinked-PR merge sweep both key off it).
          branch: z.string().max(255).optional(),
          // EXP-484: echoed so a resurrected row keeps naming its agent.
          agent: z.enum(codingAgentValues).optional(),
        })
        .refine((value) => !(value.branch && value.issueId), {
          message: `branch excludes issueId — an issue session's branch lives on the issue`,
        })
        .refine((value) => !(value.issueId && value.teamId), {
          message: `At most one of issueId/teamId`,
        })
        .refine(
          (value) =>
            !value.actionId || (Boolean(value.teamId) && !value.issueId),
          { message: `actionId requires teamId and excludes issueId` }
        )
    )
    .mutation(async ({ ctx, input }) => {
      const [existing] = await ctx.db
        .select({
          userId: codingSessions.userId,
          hostUserId: codingSessions.hostUserId,
          status: codingSessions.status,
        })
        .from(codingSessions)
        .where(eq(codingSessions.id, input.id))
        .limit(1)

      if (!existing) {
        if (!input.issueId && !input.teamId) return { alive: false }
        try {
          if (input.issueId) {
            const issueCtx = await getIssueTeamContext(input.issueId)
            await assertTeamMember(ctx.session.user.id, issueCtx.teamId)
            const attribution = await resolveStartAttribution(
              ctx.db,
              ctx.session.user.id,
              input,
              issueCtx.teamId
            )
            // A swept session may resurface AFTER its PR opened — or merged
            // (laptop suspend through the whole run) — re-derive the state
            // from the issue so the re-created row doesn't claim
            // "coding now" on a parked issue. Merge always closes (EXP-498):
            // a merged PR resurrects the row as `ended`, so the owner's
            // kill_watch tears the resumed terminal down instead of the
            // session outliving its merge.
            const [issue] = await ctx.db
              .select({ status: issues.status, prState: issues.prState })
              .from(issues)
              .where(eq(issues.id, input.issueId))
              .limit(1)
            const merged = issue?.prState === `merged`
            const device = await resolveSessionDevice(
              ctx.db,
              ctx.session.user.id,
              input
            )
            await ctx.db.insert(codingSessions).values({
              id: input.id,
              issueId: input.issueId,
              teamId: issueCtx.teamId,
              boardId: issueCtx.boardId,
              // EXP-679: an agent-started issue run echoes its reason so a
              // swept row resurrects unattended (schedule/event never reach
              // an issue subject — same rule as `start`).
              startedReason:
                input.startedReason === `agent` ? `agent` : null,
              userId: attribution.userId,
              hostUserId: attribution.hostUserId,
              ...device,
              agent: input.agent ?? null,
              status: merged
                ? `ended`
                : issue?.status === `in_review`
                  ? `in_review`
                  : `running`,
              ...(merged ? { endedAt: new Date() } : {}),
            })
          } else {
            await assertTeamMember(ctx.session.user.id, input.teamId!)
            const attribution = await resolveStartAttribution(
              ctx.db,
              ctx.session.user.id,
              input,
              input.teamId!
            )
            const device = await resolveSessionDevice(
              ctx.db,
              ctx.session.user.id,
              input
            )
            // Action rows re-create from the client snapshot. If the action
            // was deleted meanwhile, a dangling-FK insert would 23503 —
            // pre-check and degrade: action_id NULL, actionName kept
            // (exactly the shape FK SET NULL leaves on live rows). The
            // pre-check races a concurrent delete; that lands in the catch
            // below and the next ping self-heals. The action must also
            // belong to the claimed team (the same derivation `start`
            // enforces) — a cross-team actionId degrades to NULL instead of
            // planting a cross-tenant FK reference in the synced row.
            // The builtin literal is not a uuid — comparing it against the
            // uuid PK would 22P02, and its rows are actionId-NULL anyway
            // (the server constant, never client text, labels them).
            const builtin =
              input.actionId !== undefined && isBuiltinActionId(input.actionId)
            let actionId: string | null = null
            if (input.actionId && !builtin) {
              const [action] = await ctx.db
                .select({ id: actions.id, teamId: actions.teamId })
                .from(actions)
                .where(eq(actions.id, input.actionId))
                .limit(1)
              actionId =
                action && action.teamId === input.teamId ? action.id : null
            }
            await ctx.db.insert(codingSessions).values({
              id: input.id,
              teamId: input.teamId!,
              actionId,
              actionName: builtin
                ? builtinActionName(input.actionId!)
                : input.actionId
                  ? (input.actionName ?? null)
                  : null,
              // Automated-run parity with `start`: only a real action row
              // can carry a schedule/event reason (builtins/batch never
              // automate) — but EXP-679's `agent` rides every subject, so it
              // echoes unconditionally.
              startedReason:
                input.startedReason === `agent`
                  ? `agent`
                  : input.actionId && !builtin
                    ? (input.startedReason ?? null)
                    : null,
              automationId:
                input.startedReason !== `agent` &&
                input.actionId &&
                !builtin &&
                input.automationId &&
                actionId
                  ? await resolveAutomationId(ctx.db, input.automationId, actionId)
                  : null,
              userId: attribution.userId,
              hostUserId: attribution.hostUserId,
              ...device,
              agent: input.agent ?? null,
              branch: input.branch ?? null,
              // Batch/action rows have no issue to re-derive review state
              // from — a resurrected session degrades to `running` (badge
              // label only; rare suspend edge, never kills anything).
              status: `running`,
            })
          }
          return { alive: true }
        } catch {
          // Issue cascade-deleted, membership revoked, or an insert race —
          // degrade to the plain report; the next ping retries.
          return { alive: false }
        }
      }
      if (
        existing.userId !== ctx.session.user.id &&
        existing.hostUserId !== ctx.session.user.id
      ) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only the session owner can heartbeat it`,
        })
      }
      if (existing.status === `ended`) return { alive: false }

      // Status-conditioned so a heartbeat racing a kill/end can never
      // resurrect the row's freshness after it ended. The SET touches only
      // updatedAt — never status — so a ping cannot downgrade an
      // `in_review` row back to `running`. EXP-549: a ping carrying
      // the deviceId also refreshes the device stamp (id + the registry
      // label), so a rename converges within one beat even for clients that
      // only render the snapshot.
      const device = input.deviceId
        ? await resolveSessionDevice(ctx.db, ctx.session.user.id, input)
        : null
      const updated = await ctx.db
        .update(codingSessions)
        .set({ updatedAt: new Date(), ...(device ?? {}) })
        .where(
          and(
            eq(codingSessions.id, input.id),
            inArray(codingSessions.status, [`running`, `in_review`])
          )
        )
        .returning({ id: codingSessions.id })

      return { alive: updated.length > 0 }
    }),

  // Desktop-only attention flag (EXP-214): flips `needs_input` when the
  // agent parks on a plan-approval / AskUserQuestion picker (the steer
  // activity emitter's picker watchers) and clears it when the picker
  // resolves. Deliberately a separate boolean, not a status — running/
  // in_review stay server-owned and a ping can never race the PR-open flip.
  // Fire-and-forget on the client like heartbeat: failures are never thrown
  // into the terminal path. EXP-679 retired EXP-531's `running`-only fence on
  // `true`: since EXP-673 a person-started run stays LIVE after its PR opens,
  // and the post-turn idle nudge there means exactly "your turn now" — the
  // refusal pinned the clients' "Working…" indicator forever after pr_open.
  // The list-badge masking EXP-531 wanted lives in `sessionDisplayState`
  // (`in_review` beats `needsInput`), so the server no longer refuses the
  // flag: both values are accepted on every LIVE status. `ended` stays final,
  // and a refused write is a silent no-op (`updated: false`) the desktop
  // forwarder treats as delivered — never an error it would retry.
  setNeedsInput: authedProcedure
    .input(z.object({ id: z.string().uuid(), needsInput: z.boolean() }))
    .mutation(async ({ ctx, input }) => {
      const [existing] = await ctx.db
        .select({
          userId: codingSessions.userId,
          hostUserId: codingSessions.hostUserId,
          status: codingSessions.status,
        })
        .from(codingSessions)
        .where(eq(codingSessions.id, input.id))
        .limit(1)

      if (!existing) return { updated: false }
      if (
        existing.userId !== ctx.session.user.id &&
        existing.hostUserId !== ctx.session.user.id
      ) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only the session owner can update it`,
        })
      }

      // Status-conditioned like heartbeat: an ended row stays final and never
      // re-surfaces as "needs input". Every LIVE status takes both values
      // (EXP-679 — an in_review run is still a run awaiting its human).
      const updated = await ctx.db
        .update(codingSessions)
        .set({ needsInput: input.needsInput })
        .where(
          and(
            eq(codingSessions.id, input.id),
            inArray(codingSessions.status, [`running`, `in_review`])
          )
        )
        .returning({ id: codingSessions.id })

      return { updated: updated.length > 0 }
    }),

  end: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const [existing] = await ctx.db
        .select({
          id: codingSessions.id,
          userId: codingSessions.userId,
          hostUserId: codingSessions.hostUserId,
          status: codingSessions.status,
        })
        .from(codingSessions)
        .where(eq(codingSessions.id, input.id))
        .limit(1)

      if (!existing) {
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Coding session not found`,
        })
      }
      // EXP-432: the hosting daemon (owner of the shared device) operates the
      // run and must be able to end its own child's session row.
      if (
        existing.userId !== ctx.session.user.id &&
        existing.hostUserId !== ctx.session.user.id
      ) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only the session owner can end it`,
        })
      }

      // Idempotent: ending an already-ended session is a no-op.
      if (existing.status === `ended`) {
        const [row] = await ctx.db
          .select()
          .from(codingSessions)
          .where(eq(codingSessions.id, input.id))
          .limit(1)
        return { session: row }
      }

      // EXP-637: this is the CLIENT end path — the agent process exited, the
      // tab closed, or the app quit. `endedBy` records that, and the run
      // carries no agent-written summary (only
      // `exponential_sessions_end` writes that). needsInput is cleared so a
      // row parked on a picker can't end amber.
      const [session] = await ctx.db
        .update(codingSessions)
        .set({
          status: `ended`,
          endedAt: new Date(),
          endedBy: `client`,
          needsInput: false,
        })
        .where(eq(codingSessions.id, input.id))
        .returning()

      // EXP-700: a client end is an agent-started child that vanished
      // WITHOUT its close-out — tell a live parent so it is not left waiting
      // forever. Best-effort (internally caught, relay 3s-bounded).
      await notifyParentOfChildEnd(ctx.db, input.id, {
        summary: null,
        endedBy: `client`,
      })

      return { session }
    }),
})
