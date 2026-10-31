// eAgent orchestration (#EXP-1508): the state, events and effects the
// fixtures speak. One shape for both implementations: Exponential's server
// (Postgres rows) and `eagent-hub-core` (#EXP-1509, a pure reducer). Times
// are unix ms, ids follow eCore's id rule, refusals are eCore `Failure`s and
// every event is an eCore `Envelope`.

import type { Envelope, Failure } from "@exp/ecore"

export type Status = `queued` | `running` | `idle` | `ended`

export type BlockedKind = `rate_limit` | `device_offline` | `unclaimed` | `stuck` | `restarted`

/** Why a live session is held. `device_offline` and `restarted` are
 *  INTERRUPTIONS: `resume` = it was mid-turn (its device continues it),
 *  `resumeSince` = since when the device could (the resume clock). */
export interface Blocked {
  kind: BlockedKind
  since: number
  window?: string | null
  /** The device's own stamp (RFC 3339), kept verbatim; unparseable = never expires. */
  resetsAt?: string | null
  resume?: boolean
  resumeSince?: number | null
}

export interface HubSession {
  id: string
  parentId: string | null
  rootId: string
  depth: number
  teamId: string
  userId: string
  shared: boolean
  /** A team agent's conversation (persistent: it never ends). */
  agentId: string | null
  /** The agent whose session started this root. */
  startedByAgentId: string | null
  deviceId: string
  status: Status
  busy: boolean
  background: boolean
  needsInput: boolean
  blocked: Blocked | null
  /** Sessions this one watches (the bus wakes it with their transitions). */
  wakeOn: string[]
  createdAt: number
  startedAt: number | null
  endedAt: number | null
  lastTurnAt: number | null
  lastActivityAt: number | null
  updatedAt: number
}

export type QuestionCard = { kind: `question`; question: string; options?: string[]; multi?: boolean; custom?: boolean }
export type PlanCard = {
  kind: `plan`
  plan: string
  questions?: Array<{ id: string; prompt: string; options: Array<{ label: string }> }>
}
export type Card =
  | QuestionCard
  | PlanCard
  | { kind: `ask`; sessionId: string; messageId: string }
  | { kind: `report`; sessionId: string; summary: string; final: true }
  | { kind: `event`; event: string; sessionId: string; pullRequestId?: string }
  | {
      kind: `event`
      event: `trigger_fired`
      agentId: string
      triggerId: string | null
      eventId: string | null
      name: string | null
    }
  | { kind: `session` | `thread_started`; sessionId: string }

export interface HubMessage {
  id: string
  /** The conversation (session) it is in. */
  sessionId: string
  authorKind: `user` | `agent` | `system`
  /** The session that wrote it into another's conversation (the bus, a send). */
  authorSessionId: string | null
  body: string
  card: Card | null
  /** Sealed to its tree (EXP-1458): never read by the hub. */
  sealed: boolean
  /** Unconsumed input its reader's device still has to deliver. */
  pending: boolean
  /** A question/plan card's answer (or an `ask` mirror closed with it). */
  answered: boolean
  at: number
}

export interface HubDevice {
  id: string
  lastSeenAt: number
  /** The sessions its process runs (the heartbeat's claim); null = an older device that never says. */
  liveSessionIds: string[] | null
}

export interface Trigger {
  id: string
  kind: `schedule` | `event`
  name?: string
  enabled?: boolean
}

export interface HubAgent {
  id: string
  ownerId: string
  teamId: string
  /** `assistant` watches all its member's chats (its quiet-tick rule). */
  role: `agent` | `assistant`
  /** The owner's conversation session with the agent (where firings land). */
  conversationId: string
  triggers: Trigger[]
  /** triggerId → the due minute it last fired for. */
  firedAt: Record<string, number>
}

export interface HubState {
  /** The last message id handed out (`m<seq>`). */
  seq: number
  sessions: Record<string, HubSession>
  messages: HubMessage[]
  devices: Record<string, HubDevice>
  agents: Record<string, HubAgent>
}

/** Everything the hub does besides its own state: rows it posts, edits,
 *  briefs, nudges and kills. */
export type Effect =
  | {
      kind: `post`
      messageId: string
      to: string
      authorKind: HubMessage[`authorKind`]
      authorSessionId: string | null
      body: string
      card: Card | null
      sealed: boolean
      /** Pending input for `to` (its device delivers it: a wake-up). */
      waking: boolean
    }
  | { kind: `edit`; messageId: string; body: string; waking: boolean }
  /** The first input a claim delivers as the Request (null = none: "Wait for the next message."). */
  | { kind: `brief`; sessionId: string; messageId: string | null; sealed: boolean }
  | { kind: `nudge`; deviceId: string }
  | { kind: `kill`; sessionId: string }

export interface StepResult {
  sessionId?: string
  messageId?: string | null
  warning?: string
  endedIds?: string[]
}

export type Outcome =
  | { state: HubState; effects: Effect[]; result: StepResult; failure?: undefined }
  | { state: HubState; effects: []; result?: undefined; failure: Failure }

export type HubEvent = Envelope<Record<string, unknown>>
