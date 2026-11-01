// #EXP-1510: each shadowed server entrypoint as a core event (packages/eagent
// `reduce.ts` event data). A call the core does not model (a configure, an
// answer, a prompt, a message to an agent or app, a device card that is no
// question or plan) is counted skipped, never guessed.

import type { SessionsSendInput } from "@exp/db-schema/domain"
import type { HubState } from "@exp/eagent/model"
import { modelCommand } from "@/lib/sessions/send"
import { isSealedBody, isSealedCard } from "@/lib/sessions/sealed"
import { shadow, type ShadowPlan } from "./index"

const stamp = (value: string | null | undefined) => {
  const at = value ? Date.parse(value) : Number.NaN
  return Number.isFinite(at) ? at : undefined
}

/** A tRPC middleware's `next()` run under the shadow: an error result is thrown back into tRPC unchanged. */
export function shadowNext<R extends { ok: boolean }>(plan: ShadowPlan<unknown>, next: () => Promise<R>): Promise<R> {
  return shadow<R>(
    { ...plan, data: (result, error, after) => plan.data(result && result.ok ? (result as { data?: unknown }).data : undefined, error, after) },
    async () => {
      const result = await next()
      if (!result.ok) throw (result as unknown as { error: unknown }).error
      return result
    }
  )
}

export const claimPlan = (input: { sessionId: string; deviceId: string }): ShadowPlan<unknown> => ({
  rule: `device.claim`,
  scope: { sessionIds: [input.sessionId] },
  data: () => ({ sessionId: input.sessionId, deviceId: input.deviceId }),
})

export const reportPlan = (input: {
  sessionId: string
  deviceId?: string
  status?: `running` | `idle` | `ended`
  busy?: boolean
  background?: boolean
  needsInput?: boolean
  blocked?: { kind?: string | null; window?: string | null; resetsAt?: string | null; since?: string | null; handled?: boolean | null } | null
  lost?: `process_lost` | `daemon_restart`
  midTurn?: boolean
}): ShadowPlan<unknown> => ({
  rule: `device.report`,
  scope: { sessionIds: [input.sessionId] },
  data: (_result, _error, after) => {
    const row = after.sessions[input.sessionId]
    return {
      sessionId: input.sessionId,
      // An absent deviceId (older devices) is the row's own.
      deviceId: input.deviceId ?? row?.deviceId,
      ...(input.status !== undefined ? { status: input.status } : {}),
      ...(input.busy !== undefined ? { busy: input.busy } : {}),
      ...(input.background !== undefined ? { background: input.background } : {}),
      ...(input.needsInput !== undefined ? { needsInput: input.needsInput } : {}),
      ...(input.blocked !== undefined
        ? {
            blocked: input.blocked
              ? {
                  kind: input.blocked.kind,
                  window: input.blocked.window ?? null,
                  resetsAt: input.blocked.resetsAt ?? null,
                  ...(stamp(input.blocked.since) !== undefined ? { since: stamp(input.blocked.since) } : {}),
                  ...(input.blocked.handled ? { handled: true } : {}),
                }
              : null,
          }
        : {}),
      ...(input.lost ? { lost: input.lost } : {}),
      ...(input.midTurn !== undefined ? { midTurn: input.midTurn } : {}),
    }
  },
})

export const replyPlan = (input: {
  sessionId: string
  body: string
  card?: unknown
  listenerCopies?: ReadonlyArray<{ rootId: string; body: string }>
}): ShadowPlan<unknown> => ({
  rule: `device.reply`,
  scope: { sessionIds: [input.sessionId] },
  data: () => {
    const card = (input.card ?? null) as { kind?: string } | null
    if (card && card.kind !== `question` && card.kind !== `plan`) return null
    const sealed = isSealedBody(input.body) || (card !== null && isSealedCard(card))
    return {
      sessionId: input.sessionId,
      body: sealed ? `` : input.body,
      card,
      sealed,
      ...(input.listenerCopies?.length ? { copies: Object.fromEntries(input.listenerCopies.map((c) => [c.rootId, c.body])) } : {}),
    }
  },
})

export const consumePlan = (input: { sessionId: string; messageIds: string[] }): ShadowPlan<unknown> => ({
  rule: `device.consume`,
  scope: { sessionIds: [input.sessionId] },
  data: () => ({ sessionId: input.sessionId, messageIds: input.messageIds }),
})

export const heartbeatPlan = (input: {
  deviceId: string
  liveSessionIds?: ReadonlyArray<string | null>
  sessionActivity?: ReadonlyArray<{ sessionId: string; at: string } | null>
}): ShadowPlan<unknown> => ({
  rule: `device.heartbeat`,
  scope: { deviceIds: [input.deviceId] },
  data: () => ({
    deviceId: input.deviceId,
    liveSessionIds: input.liveSessionIds ? [...input.liveSessionIds] : null,
    ...(input.sessionActivity
      ? {
          activity: input.sessionActivity.flatMap((entry) =>
            entry && stamp(entry.at) !== undefined ? [{ sessionId: entry.sessionId, at: stamp(entry.at)! }] : []
          ),
        }
      : {}),
  }),
})

export const endPlan = (args: { sessionId: string; summary?: string | null; sealedReport?: unknown }): ShadowPlan<unknown> => ({
  rule: `session.end`,
  scope: { sessionIds: [args.sessionId] },
  data: () => ({
    sessionId: args.sessionId,
    summary: args.sealedReport ? null : (args.summary ?? null),
    summarySealed: !!args.sealedReport,
  }),
})

export const tickPlan = (now: Date): ShadowPlan<unknown> => ({
  rule: `watchdog.tick`,
  scope: { allLive: true },
  at: now,
  data: () => ({}),
})

/** Send fields the core does not model: their calls are skipped. */
const UNMODELED_SEND = [`answer`, `wakeOn`, `promptId`, `attachmentIds`, `seal`, `appId`, `agentId`] as const
const CONFIGURE = [`deviceId`, `cli`, `account`, `model`, `effort`, `planMode`, `computerUse`, `ultracode`, `mcpServerIds`] as const

export function sendPlan(args: {
  userId: string
  input: SessionsSendInput
  callerSessionId?: string | null
  viaMcp?: boolean
  meeting?: boolean
}): ShadowPlan<{ sessionId: string } | undefined> {
  // A `/model <name>` text is a configure, never a message.
  const input = modelCommand(args.input, args.viaMcp === true)
  const callerId = args.viaMcp ? (args.callerSessionId ?? null) : null
  const set = (key: string) => (input as Record<string, unknown>)[key] !== undefined && (input as Record<string, unknown>)[key] !== null
  const unmodeled =
    args.meeting ||
    UNMODELED_SEND.some(set) ||
    (input.sessionId ? CONFIGURE.some(set) : false) ||
    // Over tRPC a calling session only stamps `author_session_id` on a PERSON's row: the core has no such line.
    (!args.viaMcp && !!args.callerSessionId)
  const text = input.text ?? ``
  const sealed = isSealedBody(text)
  const rule = input.sessionId ? (`send.message` as const) : (`send.start` as const)
  return {
    rule,
    scope: {
      sessionIds: [input.sessionId, input.parentId].filter((id): id is string => !!id),
      callerId,
      targetId: input.sessionId ?? null,
    },
    after: (result) => ({ sessionIds: result?.sessionId ? [result.sessionId] : [] }),
    data: (result, _error, after: HubState) => {
      if (unmodeled) return null
      const base = { ...(callerId ? { callerId } : {}), text: sealed ? `` : text, sealed }
      if (input.sessionId) {
        const fresh = result?.sessionId && result.sessionId !== input.sessionId ? result.sessionId : undefined
        return { sessionId: input.sessionId, ...base, ...(fresh ? { freshId: fresh } : {}) }
      }
      const created = result?.sessionId ? after.sessions[result.sessionId] : undefined
      return {
        ...(input.parentId ? { parentId: input.parentId } : {}),
        ...(result?.sessionId ? { id: result.sessionId } : {}),
        ...base,
        // The server's inputs to the start (who, where, visibility), read off the row it created.
        userId: created?.userId ?? args.userId,
        ...(input.deviceId ? { deviceId: input.deviceId } : created ? { deviceId: created.deviceId } : {}),
        ...(created ? { shared: created.shared } : {}),
      }
    },
  }
}
