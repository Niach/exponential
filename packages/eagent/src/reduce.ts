// eAgent orchestration (#EXP-1508): the TypeScript reference of the rules
// in fixtures/*.json. `reduce(state, event)` = Exponential's server
// behaviour (lib/sessions send/bus/lifecycle/watchdog, trpc sessions
// claim/report/reply/consume, devices heartbeat, lib/agents/triggers) as a
// pure function: no I/O, no clock (the envelope's `at` is now). The Rust
// `eagent-hub-core` (#EXP-1509) replays the same fixtures.

import { failure, isValidId, type Failure } from "@exp/ecore"
import {
  askHiddenText,
  askText,
  blockedText,
  eventText,
  fill,
  prOpenedText,
  privateText,
  rules,
  scheduleText,
  silentMinutes,
  sliceUnits,
  stampMs,
  stuckText,
  unclaimedText,
  offlineText,
  unresumedText,
} from "./copy"
import type {
  Blocked,
  BlockedKind,
  Card,
  Effect,
  HubEvent,
  HubMessage,
  HubSession,
  HubState,
  Outcome,
  PlanCard,
  QuestionCard,
  StepResult,
} from "./model"

const MIN = 60_000
const HOUR = 60 * MIN
const copy = rules.copy
const INTERRUPTIONS: readonly BlockedKind[] = [`device_offline`, `restarted`]

class Refusal extends Error {
  constructor(readonly failure: Failure) {
    super(failure.message)
  }
}
const refuse = (code: Failure[`code`], message: string): never => {
  throw new Refusal(failure(code, message))
}

interface Ctx {
  state: HubState
  effects: Effect[]
  now: number
  eventId: string
}

// ── shared rules ─────────────────────────────────────────────────────────────

export const isLive = (session: Pick<HubSession, `status`>) => session.status !== `ended`
const isWorking = (session: Pick<HubSession, `status`>) => session.status === `running` || session.status === `idle`
/** #EXP-1731: a row the device writes for a CLI subagent (subagents.json). */
export const isSubagent = (session: Pick<HubSession, `subagentKey`>) => session.subagentKey != null

/** A tree's acting agent: its conversation root's agent, else the agent that started it. */
export const treeAgentId = (root: Pick<HubSession, `agentId` | `startedByAgentId`>) =>
  root.agentId ?? root.startedByAgentId ?? null

/** #EXP-1720: the teams `row`'s tree works in: its root's own team, then the root's scope teams. */
export function treeTeams(state: HubState, row: Pick<HubSession, `rootId` | `teamId`>): string[] {
  const root = state.sessions[row.rootId] ?? row
  return [...new Set([root.teamId, ...((root as HubSession).scopeTeamIds ?? [])])]
}

/** #EXP-1735: the team LEAD Assistant `row` reports to, or null. `row` works in a team whose lead is another
 *  member's Assistant than the one acting for its tree (the Assistant's children and the root sessions it started);
 *  never the Assistant's own conversation root, never a team agent's tree, never the lead's own work. */
export function leadOf(state: HubState, row: Pick<HubSession, `id` | `rootId` | `teamId`>): string | null {
  const leadId = state.leads?.[row.teamId]
  if (!leadId || state.agents[leadId]?.role !== `assistant`) return null
  const root = state.sessions[row.rootId]
  const acting = root ? treeAgentId(root) : null
  if (!acting || acting === leadId || state.agents[acting]?.role !== `assistant`) return null
  if (row.id === root!.id && root!.agentId === acting) return null
  return leadId
}

/** #EXP-1735: the lead's conversation (its root session) a transition of `child` also reaches, or null. */
function leadListener(state: HubState, child: HubSession): HubSession | null {
  const leadId = leadOf(state, child)
  const conversation = leadId ? state.sessions[state.agents[leadId]!.rootSessionId] : undefined
  return conversation && isLive(conversation) && conversation.id !== child.id ? conversation : null
}

/** A reader of `listener` may not see `origin` (a private session mirrored into a shared one or another member's). */
export const askHidden = (
  listener: Pick<HubSession, `shared` | `userId`>,
  origin: Pick<HubSession, `shared` | `userId`>
) => !origin.shared && (listener.shared || listener.userId !== origin.userId)

/** Whether an AGENT's message may reuse this root session: null = reuse, else the reason. */
export function reuseRefusal(
  root: Pick<HubSession, `status` | `needsInput` | `endedAt` | `lastTurnAt` | `lastActivityAt` | `updatedAt`>,
  children: number,
  now: number,
  awaitsCaller = false
): string | null {
  if (root.needsInput || awaitsCaller) return null
  const reuse = rules.reuse
  const age = (at: number | null) => (at === null ? 0 : now - at)
  if (root.status === `ended` && age(root.endedAt ?? root.updatedAt) >= reuse.maxEndedHours * HOUR) {
    return fill(copy.reuseEnded, { hours: reuse.maxEndedHours })
  }
  if (age(root.lastTurnAt ?? root.lastActivityAt ?? root.updatedAt) >= reuse.maxIdleHours * HOUR) {
    return fill(copy.reuseIdle, { hours: reuse.maxIdleHours })
  }
  if (children >= reuse.maxChildren) return fill(copy.reuseCrowded, { children })
  return null
}

/** The heartbeat's live list: valid ids only, each once. */
export function cleanLiveSessionIds(ids: readonly unknown[]): { ids: string[]; dropped: number } {
  const valid = ids.filter((id): id is string => typeof id === `string` && isValidId(id))
  return { ids: [...new Set(valid)], dropped: ids.length - valid.length }
}

const ordered = (state: HubState) =>
  Object.values(state.sessions).sort((a, b) => a.createdAt - b.createdAt || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))

function session(state: HubState, id: string | undefined | null): HubSession {
  const row = id ? state.sessions[id] : undefined
  if (!row) refuse(`not_found`, copy.notFound)
  return row!
}

const isPendingInput = (message: HubMessage) =>
  message.authorKind === `user` || (message.authorSessionId !== null && message.authorSessionId !== message.sessionId)

const openCards = (state: HubState, sessionId: string) =>
  state.messages.some(
    (m) => m.sessionId === sessionId && !m.answered && (m.card?.kind === `question` || m.card?.kind === `plan`)
  )

/** `blocked` cleared (only a flag of `kind`, when named) and needsInput re-derived from open cards. */
function clearBlocked(ctx: Ctx, row: HubSession, kind?: BlockedKind) {
  if (kind && row.blocked?.kind !== kind) return
  row.blocked = null
  row.needsInput = openCards(ctx.state, row.id)
  row.updatedAt = ctx.now
}

const interruption = (kind: BlockedKind, now: number, resume: boolean, resumeSince: number | null): Blocked => ({
  kind,
  since: now,
  window: null,
  resetsAt: null,
  resume,
  resumeSince,
})
const flag = (kind: BlockedKind, now: number): Blocked => ({ kind, since: now, window: null, resetsAt: null })

function post(
  ctx: Ctx,
  to: string,
  message: Omit<HubMessage, `id` | `sessionId` | `at` | `answered` | `pending`> & { waking: boolean }
): string {
  ctx.state.seq += 1
  const id = `m${ctx.state.seq}`
  const { waking, ...rest } = message
  ctx.state.messages.push({ ...rest, id, sessionId: to, pending: waking, answered: false, at: ctx.now })
  ctx.effects.push({
    kind: `post`,
    messageId: id,
    to,
    authorKind: rest.authorKind,
    authorSessionId: rest.authorSessionId,
    body: rest.body,
    card: rest.card,
    sealed: rest.sealed,
    waking,
  })
  return id
}

const systemCard = (ctx: Ctx, to: string, card: Card) =>
  post(ctx, to, { authorKind: `system`, authorSessionId: null, body: ``, card, sealed: false, waking: false })

// ── the bus ──────────────────────────────────────────────────────────────────

/** Who a transition of `child` reaches: its parent (whatever its status), then every live watcher by id. */
export function busListeners(state: HubState, child: Pick<HubSession, `id` | `parentId`>): HubSession[] {
  const parent = child.parentId ? state.sessions[child.parentId] : undefined
  const watchers = Object.values(state.sessions)
    .filter((row) => isLive(row) && row.id !== child.id && row.id !== child.parentId && row.wakeOn.includes(child.id))
    .sort((a, b) => (a.id < b.id ? -1 : 1))
  return parent ? [parent, ...watchers] : watchers
}

type Line = { body: string; card: Card | null; sealed?: boolean }

function deliver(
  ctx: Ctx,
  child: HubSession,
  message: {
    authorKind: `agent` | `system`
    body: string
    card: Card | null
    hidden?: Line
    waking: boolean
    fallbackToSelf?: boolean
    skip?: (listener: HubSession) => boolean
    /** Sealed to the child's tree: in-tree listeners get a byte copy, other trees their `copies[rootId]`, else `hidden`. */
    sealed?: { inTree?: Line; copies?: Record<string, string> }
    /** #EXP-1735: the team lead's conversation hears it too (after the others, once). */
    lead?: boolean
  }
) {
  // #EXP-1731: a subagent's parent is the CLI process that ran it and has every line in-band: the bus never posts
  // into it (no wake-up per subagent turn, not even its end report). Its watchers still hear it.
  const subagent = isSubagent(child)
  const all = busListeners(ctx.state, child).filter((listener) => !(subagent && listener.id === child.parentId))
  // #EXP-1735: the team lead hears it too (once); a subagent's lines stay inside its parent's tree.
  const lead = message.lead && !subagent ? leadListener(ctx.state, child) : null
  if (lead && !all.some((listener) => listener.id === lead.id)) all.push(lead)
  const listeners = message.skip ? all.filter((listener) => !message.skip!(listener)) : all
  for (const listener of listeners) {
    const plain: Line = { body: message.body, card: message.card }
    let line: Line = message.hidden && askHidden(listener, child) ? message.hidden : plain
    if (message.sealed && line !== message.hidden) {
      const copy = message.sealed.copies?.[listener.rootId]
      line =
        listener.rootId === child.rootId
          ? (message.sealed.inTree ?? { ...plain, sealed: true })
          : copy !== undefined
            ? { body: copy, card: message.card, sealed: true }
            : (message.hidden ?? { body: ``, card: message.card })
    }
    post(ctx, listener.id, {
      authorKind: message.authorKind,
      authorSessionId: child.id,
      body: line.body,
      card: line.card,
      sealed: line.sealed === true,
      waking: message.waking && isLive(listener),
    })
  }
  if (all.length === 0 && message.fallbackToSelf) {
    post(ctx, child.id, {
      authorKind: message.authorKind,
      authorSessionId: null,
      body: message.body,
      card: message.card,
      sealed: false,
      waking: false,
    })
  }
}

type LineEvent = `session_claimed` | `session_blocked` | `session_unclaimed` | `session_stuck` | `session_interrupted` | `pr_opened`

function busLine(
  ctx: Ctx,
  child: HubSession,
  event: LineEvent,
  body: string,
  opts: { waking: boolean; pullRequestId?: string; selfFallback?: boolean }
) {
  const card: Card = { kind: `event`, event, sessionId: child.id }
  const silent = rules.assistantSilentEvents.includes(event)
  deliver(ctx, child, {
    authorKind: `system`,
    body,
    card: opts.pullRequestId ? { ...card, pullRequestId: opts.pullRequestId } : card,
    ...(event === `pr_opened` ? { hidden: { body: privateText(child.id, copy.privatePrLead), card } } : {}),
    waking: opts.waking,
    fallbackToSelf: opts.selfFallback !== false,
    ...(silent
      ? {
          skip: (listener: HubSession) =>
            listener.agentId !== null &&
            listener.id === listener.rootId &&
            ctx.state.agents[listener.agentId]?.role === `assistant`,
        }
      : {}),
  })
}

function busRelayTurn(ctx: Ctx, child: HubSession, body: string, sealed: boolean, copies?: Record<string, string>) {
  const text = body.trim()
  if (!text) return
  deliver(ctx, child, {
    ...(sealed ? { sealed: { copies } } : {}),
    authorKind: `agent`,
    body: text,
    card: null,
    hidden: { body: privateText(child.id, copy.privateReplyLead), card: null },
    waking: true,
    // A team agent's conversation that only WATCHES the child hears its ask, wall, PR and report, never each turn;
    // #FEED-194: unless it is the agent that started the root session.
    skip: (listener) =>
      listener.agentId !== null && listener.id !== child.parentId && listener.agentId !== child.startedByAgentId,
  })
}

function busAsk(ctx: Ctx, child: HubSession, messageId: string, card: QuestionCard | PlanCard) {
  const mirror: Card = { kind: `ask`, sessionId: child.id, messageId }
  deliver(ctx, child, {
    authorKind: `agent`,
    body: askText(child.id, card),
    card: mirror,
    hidden: { body: askHiddenText(child.id, card), card: mirror },
    waking: true,
    lead: true,
  })
}

function reportSummary(ctx: Ctx, child: HubSession, summary: string | null, summarySealed: boolean): string {
  let text = summarySealed ? `` : (summary?.trim() ?? ``)
  if (!text) {
    const last = ctx.state.messages
      .filter((m) => m.sessionId === child.id && m.authorKind === `agent` && m.card === null && m.authorSessionId === null)
      .at(-1)
    text = last && !last.sealed ? last.body.trim() : ``
  }
  return sliceUnits(text || rules.report.fallback, rules.report.max)
}

function busReport(ctx: Ctx, child: HubSession, summary: string | null, summarySealed: boolean) {
  const text = reportSummary(ctx, child, summary, summarySealed)
  deliver(ctx, child, {
    authorKind: `agent`,
    body: text,
    card: { kind: `report`, sessionId: child.id, summary: text, final: true },
    hidden: {
      body: privateText(child.id, copy.privateReportLead),
      card: { kind: `report`, sessionId: child.id, summary: copy.privateReportLead, final: true },
    },
    waking: true,
    lead: true,
  })
  for (const row of Object.values(ctx.state.sessions)) row.wakeOn = row.wakeOn.filter((id) => id !== child.id)
}

/** `watcher` watches `ids`: live sessions of its tree's teams its account may see, never itself. */
function watchSessions(ctx: Ctx, watcherId: string, ids: readonly string[]): string[] {
  const watcher = ctx.state.sessions[watcherId]
  if (!watcher) return []
  const teams = treeTeams(ctx.state, watcher)
  const live = [...new Set(ids)]
    .filter((id) => id !== watcherId)
    .map((id) => ctx.state.sessions[id])
    .filter(
      (row): row is HubSession =>
        !!row && isLive(row) && teams.includes(row.teamId) && (row.shared || row.userId === watcher.userId)
    )
    .map((row) => row.id)
  watcher.wakeOn = [...new Set([...watcher.wakeOn, ...live])]
  return live
}

/** #FEED-206: a revived root is watched again, as at its start: by its starter (the live session carrying its
 *  `thread_started` card) and by the calling session outside its tree. */
function rewatchRevived(ctx: Ctx, root: HubSession, caller: HubSession | null) {
  const watchers = new Set<string>()
  for (const m of ctx.state.messages) {
    if (m.card?.kind !== `thread_started` || m.card.sessionId !== root.id) continue
    const starter = ctx.state.sessions[m.sessionId]
    if (starter && isLive(starter) && starter.rootId !== root.id) watchers.add(starter.id)
  }
  if (caller && caller.rootId !== root.id) watchers.add(caller.id)
  for (const id of [...watchers].sort()) watchSessions(ctx, id, [root.id])
}

// ── the tree ─────────────────────────────────────────────────────────────────

function assertTreeRoom(state: HubState, rootId: string, depth: number) {
  if (depth > rules.caps.treeMaxDepth) refuse(`invalid`, fill(copy.tooDeep, { max: rules.caps.treeMaxDepth }))
  // #EXP-1731: subagent rows hold no slot (the CLI already ran them; the caps stay today's).
  const live = Object.values(state.sessions).filter((row) => row.rootId === rootId && isLive(row) && !isSubagent(row)).length
  if (live >= rules.caps.treeMaxSessions) refuse(`limit`, fill(copy.tooMany, { max: rules.caps.treeMaxSessions }))
}

function insertSession(ctx: Ctx, row: Pick<HubSession, `id` | `parentId` | `rootId` | `depth` | `teamId` | `userId` | `shared` | `deviceId`> & Partial<HubSession>): HubSession {
  if (!isValidId(row.id)) refuse(`invalid`, `Invalid session id`)
  if (ctx.state.sessions[row.id]) refuse(`conflict`, `Session ${row.id} exists`)
  const created: HubSession = {
    agentId: null,
    startedByAgentId: null,
    continuedFromId: null,
    subagentKey: null,
    status: `queued`,
    busy: false,
    background: false,
    needsInput: false,
    blocked: null,
    wakeOn: [],
    sealEpoch: null,
    scopeTeamIds: null,
    createdAt: ctx.now,
    startedAt: null,
    endedAt: null,
    lastTurnAt: null,
    lastActivityAt: null,
    updatedAt: ctx.now,
    ...row,
  }
  ctx.state.sessions[row.id] = created
  return created
}

/** A send's message: the caller session's input (agent-authored) or a person's; blank = none. */
function postSend(ctx: Ctx, to: HubSession, text: string, sealed: boolean, caller: HubSession | null): string | null {
  if (!text.trim()) return null
  // EXP-1472/#FEED-185: a keyed tree takes no plaintext, not even from its own agent (the hub never reads a sealed
  // tree); the writer's device seals it (its MCP gateway) or the send fails.
  if (!sealed && ctx.state.sessions[to.rootId]?.sealEpoch != null) refuse(`invalid`, copy.plainInSealedTree)
  return post(ctx, to.id, {
    authorKind: caller ? `agent` : `user`,
    authorSessionId: caller?.id ?? null,
    body: text,
    card: null,
    sealed,
    waking: true,
  })
}

const callerTreeAgent = (state: HubState, caller: HubSession | null) =>
  caller ? treeAgentId(state.sessions[caller.rootId] ?? caller) : null

interface SendData {
  id?: string
  parentId?: string
  sessionId?: string
  callerId?: string
  userId?: string
  deviceId?: string
  shared?: boolean
  text?: string
  sealed?: boolean
  /** #FEED-173: what a new root session works on when it has no text: issues, a prompt, attachments. */
  issueIds?: string[]
  promptId?: string
  attachmentIds?: string[]
  /** The id a fresh root session or a continuation takes. */
  freshId?: string
  /** #EXP-1720: the team a new session starts in (one of the caller's / parent's tree teams). */
  teamId?: string
  /** #EXP-1720: the Assistant's teams for this turn (a message to its conversation root): the root's new scope. */
  scopeTeamIds?: string[]
}

/** #EXP-1720: the team a new session of a tree starts in: the named one when the tree works there, else refused. */
function startTeam(state: HubState, tree: HubSession | null, teamId: string | undefined, fallback: string): string {
  if (!teamId || teamId === fallback) return fallback
  if (tree && !treeTeams(state, tree).includes(teamId)) refuse(`forbidden`, copy.teamOutOfScope)
  return teamId
}

function startRoot(ctx: Ctx, data: SendData, caller: HubSession | null): StepResult {
  const callerRoot = caller ? (ctx.state.sessions[caller.rootId] ?? null) : null
  const deviceId = data.deviceId ?? (callerRoot && treeAgentId(callerRoot) ? callerRoot.deviceId : undefined)
  if (!deviceId) refuse(`invalid`, `Pick a device for the new session`)
  // #FEED-173: a start with nothing to work on (no text, issues, prompt or attachments) is refused, never an empty session.
  if (!data.text?.trim() && !data.sealed && !data.issueIds?.length && !data.promptId && !data.attachmentIds?.length)
    refuse(`invalid`, copy.startNeedsWork)
  const id = data.id ?? data.freshId ?? ``
  const root = insertSession(ctx, {
    id,
    parentId: null,
    rootId: id,
    depth: 0,
    teamId:
      caller && callerTreeAgent(ctx.state, caller)
        ? startTeam(ctx.state, caller, data.teamId, caller.teamId)
        : (data.teamId ?? caller?.teamId ?? rules.defaults.session.teamId),
    userId: data.userId ?? caller?.userId ?? rules.defaults.session.userId,
    shared: data.shared ?? true,
    deviceId: deviceId!,
    startedByAgentId: callerTreeAgent(ctx.state, caller),
  })
  // #FEED-185: a sealed start keys its new tree (the server creates its key at epoch 1, createTreeKeyInTx).
  if (data.sealed === true) root.sealEpoch = 1
  const messageId = postSend(ctx, root, data.text ?? ``, data.sealed === true, caller)
  if (caller) {
    watchSessions(ctx, caller.id, [root.id])
    systemCard(ctx, caller.id, { kind: `thread_started`, sessionId: root.id })
  }
  return { sessionId: root.id, messageId }
}

function startChild(ctx: Ctx, data: SendData, caller: HubSession | null): StepResult {
  const parent = session(ctx.state, data.parentId)
  if (isSubagent(parent)) refuse(`invalid`, copy.subagentNoChildren)
  assertTreeRoom(ctx.state, parent.rootId, parent.depth + 1)
  const root = session(ctx.state, parent.rootId)
  const child = insertSession(ctx, {
    id: data.id ?? ``,
    parentId: parent.id,
    rootId: parent.rootId,
    depth: parent.depth + 1,
    teamId: startTeam(ctx.state, parent, data.teamId, parent.teamId),
    userId: data.userId ?? caller?.userId ?? parent.userId,
    shared: root.shared,
    deviceId: data.deviceId ?? parent.deviceId,
  })
  const messageId = postSend(ctx, child, data.text ?? ``, data.sealed === true, caller)
  systemCard(ctx, parent.id, { kind: `session`, sessionId: child.id })
  return { sessionId: child.id, messageId }
}

/** #FEED-172: the target's tree has the last word with the caller: its newest agent-authored line in the caller's
 *  conversation is newer than the caller's newest message into the target's tree. A reply never forks. */
export function awaitsCaller(state: HubState, caller: HubSession, target: HubSession): boolean {
  const inTree = (id: string | null) => !!id && state.sessions[id]?.rootId === target.rootId
  let from = -Infinity
  let to = -Infinity
  for (const m of state.messages) {
    if (m.sessionId === caller.id && m.authorKind === `agent` && inTree(m.authorSessionId)) from = Math.max(from, m.at)
    if (m.authorSessionId === caller.id && inTree(m.sessionId)) to = Math.max(to, m.at)
  }
  return from > -Infinity && from > to
}

function agentFreshSession(ctx: Ctx, data: SendData, caller: HubSession | null, target: HubSession): StepResult | null {
  if (!caller || caller.rootId === target.rootId) return null
  if (target.parentId !== null || target.agentId) return null
  if (!data.text?.trim() || data.sealed) return null
  if (target.needsInput) return null
  if (!callerTreeAgent(ctx.state, caller)) return null
  const children = Object.values(ctx.state.sessions).filter(
    (row) => row.rootId === target.id && row.id !== target.id && !isSubagent(row)
  ).length
  const reason = reuseRefusal(target, children, ctx.now, awaitsCaller(ctx.state, caller, target))
  if (!reason) return null
  const outcome = startRoot(ctx, { ...data, id: data.freshId, deviceId: data.deviceId ?? target.deviceId }, caller)
  return { ...outcome, warning: fill(copy.freshSession, { id: target.id, reason }) }
}

function sendMessage(ctx: Ctx, data: SendData, caller: HubSession | null): StepResult {
  const target = session(ctx.state, data.sessionId)
  if (isSubagent(target) && target.parentId) {
    // #EXP-1731: a subagent runs inside its parent's process and takes no input of its own (a message mid-run would
    // resume a second copy): the message goes to its parent instead, exactly as if sent there, with a warning.
    const out = sendMessage(ctx, { ...data, sessionId: target.parentId }, caller)
    const redirect = fill(copy.subagentRedirect, { id: target.id, parentId: target.parentId })
    return { ...out, warning: out.warning ? `${redirect} ${out.warning}` : redirect }
  }
  if (data.scopeTeamIds) {
    // #EXP-1720: a turn's teams are the Assistant's alone: they become its conversation root's scope.
    const assistant = target.agentId ? ctx.state.agents[target.agentId] : undefined
    if (target.parentId !== null || assistant?.role !== `assistant`) refuse(`invalid`, copy.scopeAssistantOnly)
    target.scopeTeamIds = [...new Set(data.scopeTeamIds.filter((id) => id !== target.teamId))]
    target.updatedAt = ctx.now
  }
  if (caller) {
    const fresh = agentFreshSession(ctx, data, caller, target)
    if (fresh) return fresh
  }
  const text = data.text ?? ``
  if (isLive(target) || target.agentId) {
    const messageId = postSend(ctx, target, text, data.sealed === true, caller)
    return { sessionId: target.id, messageId }
  }
  if (!text.trim()) refuse(`invalid`, copy.endedNeedsMessage)
  if (target.parentId === null) {
    // An ended ROOT revives in place: the same row idles with the message pending.
    Object.assign(target, {
      status: `idle`,
      endedAt: null,
      busy: false,
      background: false,
      needsInput: false,
      blocked: null,
      updatedAt: ctx.now,
    })
    rewatchRevived(ctx, target, caller)
    const messageId = postSend(ctx, target, text, data.sealed === true, caller)
    return { sessionId: target.id, messageId }
  }
  // An ended CHILD continues as its sibling: same parent and depth.
  assertTreeRoom(ctx.state, target.rootId, target.depth || 1)
  const root = session(ctx.state, target.rootId)
  const child = insertSession(ctx, {
    id: data.freshId ?? ``,
    parentId: target.parentId,
    rootId: target.rootId,
    depth: target.depth || 1,
    teamId: target.teamId,
    userId: data.userId ?? caller?.userId ?? target.userId,
    shared: root.shared,
    deviceId: data.deviceId ?? target.deviceId,
    continuedFromId: target.id,
  })
  const messageId = postSend(ctx, child, text, data.sealed === true, caller)
  systemCard(ctx, target.parentId, { kind: `session`, sessionId: child.id })
  return { sessionId: child.id, messageId }
}

/** The session and its live descendants (a root = its whole tree), deepest first, oldest first. */
function liveSubtree(state: HubState, top: HubSession): HubSession[] {
  const rows =
    top.parentId === null
      ? Object.values(state.sessions).filter((row) => row.rootId === top.id)
      : (() => {
          const out: HubSession[] = []
          const walk = (row: HubSession) => {
            out.push(row)
            for (const kid of Object.values(state.sessions)) if (kid.parentId === row.id) walk(kid)
          }
          walk(top)
          return out
        })()
  return rows
    .filter(isLive)
    .sort((a, b) => b.depth - a.depth || a.createdAt - b.createdAt || (a.id < b.id ? -1 : 1))
}

/** Each row ends (never an agent's conversation), closes its ask mirrors, says `session_ended` on itself and
 *  reports into its parent and watchers; `named` carries the summary. */
function endRows(ctx: Ctx, rows: HubSession[], named?: { id: string; summary: string | null; summarySealed: boolean }): string[] {
  const ended: string[] = []
  for (const row of rows) {
    if (row.agentId || !isLive(row)) continue
    Object.assign(row, {
      status: `ended`,
      endedAt: ctx.now,
      busy: false,
      background: false,
      needsInput: false,
      blocked: null,
      updatedAt: ctx.now,
    })
    for (const m of ctx.state.messages) {
      if (m.card?.kind === `ask` && m.card.sessionId === row.id && !m.answered) m.answered = true
    }
    ended.push(row.id)
    systemCard(ctx, row.id, { kind: `event`, event: `session_ended`, sessionId: row.id })
    const own = named?.id === row.id
    busReport(ctx, row, own ? named!.summary : null, own ? named!.summarySealed : false)
  }
  // #EXP-1731: a subagent has no process of its own to kill (its parent's runs it).
  for (const id of ended) if (!isSubagent(ctx.state.sessions[id]!)) ctx.effects.push({ kind: `kill`, sessionId: id })
  return ended
}

/** #EXP-1731: a subagent row's end: its summary (a failure says so) as its own last line, then End (no kill, no
 *  report into its parent: deliver() skips it). */
function endSubagent(ctx: Ctx, row: HubSession, summary: string | null, failed: boolean): string[] {
  // #FEED-192: a keyed tree never takes the device's plaintext summary (dropped, never stored).
  const said = ctx.state.sessions[row.rootId]?.sealEpoch != null ? `` : (summary?.trim() ?? ``)
  const text = failed ? (said ? fill(copy.subagentFailed, { summary: said }) : copy.subagentFailedBare) : said
  if (text) post(ctx, row.id, { authorKind: `agent`, authorSessionId: null, body: text, card: null, sealed: false, waking: false })
  return endRows(ctx, [row], { id: row.id, summary: text || null, summarySealed: false })
}

/** #EXP-1731: a parent whose turn is over (not busy: background work included) runs no subagent any more; the
 *  ones its device never closed end now, oldest first. */
function sweepSubagents(ctx: Ctx, parent: HubSession): string[] {
  if (parent.busy && isLive(parent)) return []
  return ordered(ctx.state)
    .filter((row) => row.parentId === parent.id && isSubagent(row) && isLive(row))
    .flatMap((row) => endSubagent(ctx, row, null, false))
}

interface SubagentData {
  sessionId?: string
  deviceId?: string
  key?: string
  freshId?: string
  status?: `running` | `ended`
  failed?: boolean
  summary?: string | null
}

/** #EXP-1731: the device's subagent edge. The first `running` (or a late first `ended`) of a key under a parent
 *  writes the row: one below the parent, its team, member, visibility and device, running and busy at once (never
 *  queued, no brief, no first input, no card on the parent); later `running` edges are its progress, `ended` its End.
 *  Every edge is a sign of life of the parent too. */
function subagentEdge(ctx: Ctx, data: SubagentData): StepResult {
  const parent = session(ctx.state, data.sessionId)
  const key = data.key?.trim() ?? ``
  if (!key) refuse(`invalid`, copy.subagentNoKey)
  if (isSubagent(parent)) refuse(`invalid`, copy.subagentNoChildren)
  // Like a report: a parent that ended, or a device it left, is ignored.
  if (!isLive(parent) || (data.deviceId !== undefined && data.deviceId !== parent.deviceId)) return {}
  parent.lastActivityAt = ctx.now
  let row = ordered(ctx.state).find((r) => r.parentId === parent.id && r.subagentKey === key)
  if (!row) {
    row = insertSession(ctx, {
      id: data.freshId ?? ``,
      parentId: parent.id,
      rootId: parent.rootId,
      depth: parent.depth + 1,
      teamId: parent.teamId,
      userId: parent.userId,
      shared: parent.shared,
      deviceId: parent.deviceId,
      subagentKey: key,
      status: `running`,
      busy: true,
      startedAt: ctx.now,
    })
  } else if (data.status !== `ended` && !isLive(row)) {
    // A resumed subagent (its CLI ran the same key again) is live again in place.
    Object.assign(row, { status: `running`, busy: true, endedAt: null, updatedAt: ctx.now })
  }
  row.lastActivityAt = ctx.now
  if (data.status === `ended` && isLive(row)) {
    return { sessionId: row.id, endedIds: endSubagent(ctx, row, data.summary ?? null, data.failed === true) }
  }
  return { sessionId: row.id }
}

// ── the device ───────────────────────────────────────────────────────────────

function claim(ctx: Ctx, data: { sessionId?: string; deviceId?: string }): StepResult {
  const row = session(ctx.state, data.sessionId)
  if (data.deviceId !== row.deviceId) refuse(`conflict`, copy.otherDevice)
  if (!isLive(row)) refuse(`conflict`, copy.claimEnded)
  const queued = row.status === `queued`
  const first = queued && row.startedAt === null
  if (queued) row.status = `running`
  row.startedAt = row.startedAt ?? ctx.now
  if (row.blocked?.kind === `unclaimed`) {
    row.blocked = null
    row.needsInput = false
  }
  row.updatedAt = ctx.now
  if (first) {
    // The brief's Request IS the first input (a person's or the starting agent's, sealed or not): the claim
    // delivers it, so the device never feeds it twice and never starts on an empty brief.
    const input = ctx.state.messages.find((m) => m.sessionId === row.id && m.card === null && isPendingInput(m))
    if (input) input.pending = false
    ctx.effects.push({ kind: `brief`, sessionId: row.id, messageId: input?.id ?? null, sealed: input?.sealed ?? false })
  }
  if (queued) busLine(ctx, row, `session_claimed`, copy.claimed, { waking: false, selfFallback: false })
  return { sessionId: row.id }
}

interface ReportData {
  sessionId?: string
  deviceId?: string
  status?: `running` | `idle` | `ended`
  busy?: boolean
  background?: boolean
  needsInput?: boolean
  blocked?: (Omit<Blocked, `since`> & { since?: number; handled?: boolean }) | null
  lost?: `process_lost` | `daemon_restart`
  midTurn?: boolean
}

function report(ctx: Ctx, data: ReportData): StepResult {
  const row = ctx.state.sessions[data.sessionId ?? ``]
  // An ended row stays ended; a device the session moved away from is ignored.
  if (!row || !isLive(row) || (data.deviceId !== undefined && data.deviceId !== row.deviceId)) return {}
  const midTurn = data.midTurn ?? (row.busy || row.status === `running`)
  if (data.lost) {
    // The process went away without the agent finishing: never "done", never ended. Mid-turn = the silent
    // `restarted` interruption (its device continues it; the watchdog's resume rule catches one it does not).
    const flaggable = row.blocked === null || row.blocked.kind === `stuck` || INTERRUPTIONS.includes(row.blocked.kind)
    if (row.blocked?.kind === `stuck`) clearBlocked(ctx, row, `stuck`)
    Object.assign(row, {
      status: `idle`,
      busy: false,
      background: false,
      ...(midTurn && flaggable ? { blocked: interruption(`restarted`, ctx.now, true, ctx.now) } : {}),
      lastActivityAt: ctx.now,
      updatedAt: ctx.now,
    })
    const swept = sweepSubagents(ctx, row)
    return swept.length ? { sessionId: row.id, endedIds: swept } : { sessionId: row.id }
  }
  // A ROOT that finished a turn and an agent's conversation survive their process (idle);
  // a child, or a root that never finished a turn (a refused launch), ends.
  const status =
    data.status === `ended` && (row.agentId !== null || (row.parentId === null && row.lastTurnAt !== null))
      ? `idle`
      : data.status
  if (status === `ended`) {
    row.busy = false
    const swept = sweepSubagents(ctx, row)
    const endedIds = [...swept, ...endRows(ctx, [row])]
    return { sessionId: row.id, endedIds }
  }
  const { handled = false, ...wall } = data.blocked ?? {}
  const blocked: Blocked | null | undefined =
    data.blocked === undefined ? undefined : data.blocked === null ? null : ({ since: ctx.now, ...wall } as Blocked)
  // EXP-1536: a failed start is never said as a wall (its reply already reached the sender);
  // the next report without `blocked` (the relaunch) clears it like `stuck` and the interruptions.
  const newlyBlocked = !!blocked && !row.blocked && !handled && blocked.kind !== `start_failed`
  const nextBlocked =
    data.blocked !== undefined
      ? (blocked ?? null)
      : row.blocked && (row.blocked.kind === `stuck` || row.blocked.kind === `start_failed` || INTERRUPTIONS.includes(row.blocked.kind))
        ? null
        : row.blocked
  const turnEnded = (data.busy === false && row.busy) || status === `idle`
  const wasBlocked = row.blocked
  const needsInput =
    nextBlocked !== null ? true : data.needsInput !== undefined ? data.needsInput : wasBlocked !== null ? false : undefined
  if (status) row.status = status
  if (data.busy !== undefined) row.busy = data.busy
  else if (status === `idle`) row.busy = false
  if (data.busy === false || (data.busy === undefined && status === `idle`)) row.background = false
  else if (data.background !== undefined) row.background = data.background
  if (needsInput !== undefined) row.needsInput = needsInput
  row.blocked = nextBlocked
  if (turnEnded) row.lastTurnAt = ctx.now
  row.lastActivityAt = ctx.now
  row.updatedAt = ctx.now
  if (wasBlocked !== null && nextBlocked === null) clearBlocked(ctx, row)
  if (newlyBlocked) busLine(ctx, row, `session_blocked`, blockedText(blocked!), { waking: true })
  const swept = isSubagent(row) ? [] : sweepSubagents(ctx, row)
  return swept.length ? { sessionId: row.id, endedIds: swept } : { sessionId: row.id }
}

interface ReplyData {
  sessionId?: string
  body?: string
  card?: QuestionCard | PlanCard | null
  sealed?: boolean
  copies?: Record<string, string>
}

function reply(ctx: Ctx, data: ReplyData): StepResult {
  const row = ctx.state.sessions[data.sessionId ?? ``]
  if (!row) return {}
  const body = data.body ?? ``
  const card = data.card ?? null
  if (!card && !body.trim()) refuse(`invalid`, copy.replyEmpty)
  if (card && card.kind !== `question` && card.kind !== `plan`) refuse(`invalid`, copy.replyCard)
  const messageId = post(ctx, row.id, {
    authorKind: `agent`,
    authorSessionId: null,
    body,
    card,
    sealed: data.sealed === true,
    waking: false,
  })
  row.lastActivityAt = ctx.now
  if (card) {
    row.needsInput = true
    row.updatedAt = ctx.now
    busAsk(ctx, row, messageId, card)
  } else {
    busRelayTurn(ctx, row, body, data.sealed === true, data.copies)
  }
  return { sessionId: row.id, messageId }
}

function consume(ctx: Ctx, data: { sessionId?: string; messageIds?: string[] }): StepResult {
  const ids = new Set(data.messageIds ?? [])
  for (const m of ctx.state.messages) {
    if (m.sessionId === data.sessionId && ids.has(m.id) && m.pending) m.pending = false
  }
  return { sessionId: data.sessionId }
}

interface BeatData {
  deviceId?: string
  liveSessionIds?: unknown[] | null
  activity?: Array<{ sessionId: string; at: number }>
}

function heartbeat(ctx: Ctx, data: BeatData): StepResult {
  const deviceId = data.deviceId ?? ``
  const live = data.liveSessionIds ? cleanLiveSessionIds(data.liveSessionIds).ids : null
  ctx.state.devices[deviceId] = { id: deviceId, lastSeenAt: ctx.now, liveSessionIds: live }
  // #EXP-1731: a subagent lives and dies with its parent's process, never in the device's live list.
  const hosted = ordered(ctx.state).filter((row) => row.deviceId === deviceId && !isSubagent(row))
  // An offline-parked session: a listed one survived (busy again), an unlisted mid-turn one waits for its
  // device to continue it from now on (`resumeSince`), anything else is simply live again.
  for (const row of hosted) {
    if (row.blocked?.kind !== `device_offline`) continue
    const resume = row.blocked.resume === true
    if (!live || !resume) clearBlocked(ctx, row, `device_offline`)
    else if (live.includes(row.id)) {
      Object.assign(row, { status: `running`, busy: true, background: false, updatedAt: ctx.now })
      clearBlocked(ctx, row, `device_offline`)
    } else if (row.blocked.resumeSince == null) {
      row.blocked = { ...row.blocked, resumeSince: ctx.now }
      row.updatedAt = ctx.now
    }
  }
  const latest = new Map<string, number>()
  for (const entry of data.activity ?? []) {
    if (!isValidId(entry.sessionId) || !Number.isFinite(entry.at)) continue
    latest.set(entry.sessionId, Math.max(latest.get(entry.sessionId) ?? 0, Math.min(entry.at, ctx.now)))
  }
  for (const [id, at] of latest) {
    const row = ctx.state.sessions[id]
    if (row && row.deviceId === deviceId && isWorking(row) && (row.lastActivityAt ?? -Infinity) < at) row.lastActivityAt = at
  }
  if (live) {
    // A busy session the host no longer runs: its process died under a living device (silent).
    for (const row of hosted) {
      if (row.status !== `running` || !row.busy || (live.length > 0 && live.includes(row.id))) continue
      Object.assign(row, {
        status: `idle`,
        busy: false,
        background: false,
        blocked: interruption(`restarted`, ctx.now, true, ctx.now),
        updatedAt: ctx.now,
      })
    }
    for (const row of hosted) {
      if (live.includes(row.id) && isWorking(row) && row.blocked?.kind === `stuck`) clearBlocked(ctx, row, `stuck`)
    }
  }
  return {}
}

// ── the watchdog ─────────────────────────────────────────────────────────────

const online = (ctx: Ctx, deviceId: string) => {
  const device = ctx.state.devices[deviceId]
  return !!device && device.lastSeenAt >= ctx.now - rules.watchdog.onlineWindowSeconds * 1000
}
/** #FEED-210: the run itself spoke to the server inside the online window
 *  (an MCP call, a push: `lastActivityAt`): it and its device are up,
 *  whatever the device's beat says. */
const spokeLately = (ctx: Ctx, row: HubSession) =>
  row.lastActivityAt != null &&
  row.lastActivityAt >= ctx.now - rules.watchdog.onlineWindowSeconds * 1000 &&
  row.lastActivityAt <= ctx.now
const liveness = (row: HubSession) => Math.max(row.updatedAt, row.lastActivityAt ?? row.updatedAt)

function tick(ctx: Ctx): StepResult {
  const w = rules.watchdog
  const now = ctx.now
  // #EXP-1731: the watchdog never flags a subagent row: its liveness is its parent's (sweepSubagents ends strays).
  const watched = ordered(ctx.state).filter((row) => !isSubagent(row))
  // Unclaimed: queued past unclaimedMinutes, flagged once.
  for (const row of watched) {
    if (row.status !== `queued` || row.startedAt !== null || row.blocked || !(row.createdAt < now - w.unclaimedMinutes * MIN)) continue
    Object.assign(row, { blocked: flag(`unclaimed`, now), needsInput: true, updatedAt: now })
    busLine(ctx, row, `session_unclaimed`, unclaimedText(), { waking: true })
  }
  // Stalled input: its host is nudged again every tick while it lasts.
  const nudged = new Set<string>()
  for (const row of watched) {
    if (!isWorking(row) || row.blocked?.kind === `device_offline` || !ctx.state.devices[row.deviceId]) continue
    const stalled = ctx.state.messages.some(
      (m) => m.sessionId === row.id && m.pending && m.at < now - w.nudgeMinutes * MIN && isPendingInput(m)
    )
    if (stalled && !nudged.has(row.deviceId)) {
      nudged.add(row.deviceId)
      ctx.effects.push({ kind: `nudge`, deviceId: row.deviceId })
    }
  }
  // Stuck: busy with no sign of life, unless its online host lists it live.
  for (const row of watched) {
    const alive = liveness(row)
    if (!isWorking(row) || !row.busy || row.blocked) continue
    if (row.background && !(alive < now - w.backgroundStuckMinutes * MIN)) continue
    if (!(alive < now - w.stuckMinutes * MIN)) continue
    if (online(ctx, row.deviceId) && ctx.state.devices[row.deviceId]!.liveSessionIds?.includes(row.id)) continue
    Object.assign(row, { blocked: flag(`stuck`, now), needsInput: true, updatedAt: now })
    busLine(ctx, row, `session_stuck`, stuckText(silentMinutes(alive, now)), { waking: true })
  }
  // Offline host: a busy run is interrupted (resume, silent); a claimed one between turns just reads idle.
  for (const row of watched) {
    if (!isWorking(row) || !row.busy || row.blocked || online(ctx, row.deviceId) || spokeLately(ctx, row)) continue
    Object.assign(row, {
      status: `idle`,
      busy: false,
      background: false,
      blocked: interruption(`device_offline`, now, true, null),
      updatedAt: now,
    })
  }
  for (const row of watched) {
    if (row.status === `running` && !row.busy && !online(ctx, row.deviceId)) Object.assign(row, { status: `idle`, updatedAt: now })
  }
  // Unresumed: an interruption its device has not continued resumeMinutes after it could = on a person, said once.
  for (const row of watched) {
    const b = row.blocked
    if (!isWorking(row) || !b || !INTERRUPTIONS.includes(b.kind) || row.needsInput || b.resume !== true) continue
    if (b.resumeSince == null || !(b.resumeSince < now - w.resumeMinutes * MIN)) continue
    Object.assign(row, { needsInput: true, updatedAt: now })
    busLine(ctx, row, `session_interrupted`, unresumedText(), { waking: true })
  }
  // Offline too long: a cut run whose device never came back = on a person, said once.
  for (const row of watched) {
    const b = row.blocked
    if (!isWorking(row) || b?.kind !== `device_offline` || row.needsInput || b.resume !== true) continue
    if (b.resumeSince != null || !(b.since < now - w.offlineMinutes * MIN)) continue
    Object.assign(row, { needsInput: true, updatedAt: now })
    busLine(ctx, row, `session_interrupted`, offlineText(), { waking: true })
  }
  // A usage wall whose reset passed: cleared, silently.
  for (const row of watched) {
    if (!isWorking(row) || row.blocked?.kind !== `rate_limit` || !row.blocked.resetsAt) continue
    const at = stampMs(row.blocked.resetsAt)
    if (Number.isFinite(at) && at <= now) clearBlocked(ctx, row, `rate_limit`)
  }
  return {}
}

// ── wake-ups: schedules and events ───────────────────────────────────────────

function treesMoved(ctx: Ctx, agentId: string, since: number): boolean {
  const agent = ctx.state.agents[agentId]!
  // #EXP-1720: an Assistant's chats span its conversation's teams.
  const conversation = ctx.state.sessions[agent.rootSessionId]
  const teams = agent.role === `assistant` && conversation ? treeTeams(ctx.state, conversation) : [agent.teamId]
  const rows = Object.values(ctx.state.sessions).filter((row) => teams.includes(row.teamId))
  const moved = (row: HubSession) => row.lastActivityAt !== null && row.lastActivityAt > since
  if (agent.role === `assistant`) {
    // An Assistant watches all its member's chats; its own conversation's turns are not news, its children are.
    // #EXP-1735: a team's lead also watches the work that reports to it.
    return (
      rows.some((row) => row.userId === agent.ownerId && (row.agentId !== agent.id || row.id !== row.rootId) && moved(row)) ||
      Object.values(ctx.state.sessions).some((row) => moved(row) && leadOf(ctx.state, row) === agent.id)
    )
  }
  const roots = new Set(
    rows.filter((row) => row.id === row.rootId && (row.startedByAgentId === agentId || row.agentId === agentId)).map((row) => row.id)
  )
  const tree = rows.filter(
    (row) => roots.has(row.rootId) && (ctx.state.sessions[row.rootId]!.startedByAgentId === agentId || row.id !== row.rootId)
  )
  if (tree.length === 0) return true
  return tree.some(moved)
}

interface ScheduleData {
  agentId?: string
  due?: Array<{ triggerId: string; cron: string; tz?: string | null; at: number }>
}

function schedule(ctx: Ctx, data: ScheduleData): StepResult {
  const agent = ctx.state.agents[data.agentId ?? ``]
  if (!agent) refuse(`not_found`, `Agent not found`)
  const schedules = agent!.triggers.filter((t) => t.kind === `schedule` && t.enabled !== false)
  const due = (data.due ?? []).filter((d) => schedules.some((t) => t.id === d.triggerId))
  const fired = schedules.map((t) => agent!.firedAt[t.id]).filter((at): at is number => typeof at === `number`)
  const lastFired = fired.length ? Math.max(...fired) : null
  if (due.length === 0 || (lastFired !== null && !treesMoved(ctx, agent!.id, lastFired))) return {}
  const body = due
    .map((d) => scheduleText(d.cron, d.tz ?? null, ctx.now - d.at > rules.trigger.caughtUpAfterMinutes * MIN))
    .join(`\n`)
  const conversation = session(ctx.state, agent!.rootSessionId)
  // Never stacks: an unread tick takes the newer text; a read one nobody answered is re-armed; else a new card.
  const tick = ctx.state.messages
    .filter(
      (m) =>
        m.sessionId === conversation.id &&
        m.card?.kind === `event` &&
        m.card.event === `trigger_fired` &&
        `eventId` in m.card &&
        m.card.eventId === null &&
        m.card.triggerId !== null
    )
    .at(-1)
  const answered = tick && ctx.state.messages.some((m) => m.sessionId === conversation.id && m.authorKind === `user` && m.id !== tick.id && m.at > tick.at)
  let messageId: string
  if (tick && (tick.pending || !answered)) {
    const rearm = !tick.pending
    tick.body = body
    tick.pending = true
    messageId = tick.id
    ctx.effects.push({ kind: `edit`, messageId: tick.id, body, waking: true })
    if (rearm) ctx.effects.push({ kind: `nudge`, deviceId: conversation.deviceId })
  } else {
    messageId = post(ctx, conversation.id, {
      authorKind: `user`,
      authorSessionId: null,
      body,
      card: { kind: `event`, event: `trigger_fired`, agentId: agent!.id, triggerId: due[0]!.triggerId, eventId: null, name: null },
      sealed: false,
      waking: true,
    })
    ctx.effects.push({ kind: `nudge`, deviceId: conversation.deviceId })
  }
  for (const d of due) agent!.firedAt[d.triggerId] = d.at
  return { sessionId: conversation.id, messageId }
}

interface EmittedData {
  name?: string
  teamId?: string
  sessionId?: string | null
  emittedBy?: `session` | `user` | `server`
  payload?: unknown
}

function emitted(ctx: Ctx, data: EmittedData, eventId: string): StepResult {
  const name = data.name ?? ``
  const teamId = data.teamId ?? rules.defaults.session.teamId
  const origin = data.sessionId ? ctx.state.sessions[data.sessionId] : undefined
  const emitter = origin ? treeAgentId(ctx.state.sessions[origin.rootId] ?? origin) : null
  const agents = Object.values(ctx.state.agents).sort((a, b) => (a.id < b.id ? -1 : 1))
  for (const agent of agents) {
    if (agent.teamId !== teamId || agent.id === emitter) continue
    for (const trigger of agent.triggers) {
      if (trigger.kind !== `event` || trigger.enabled === false || trigger.name !== name) continue
      const conversation = session(ctx.state, agent.rootSessionId)
      post(ctx, conversation.id, {
        authorKind: `user`,
        authorSessionId: null,
        body: eventText(name, data.emittedBy ?? `server`, data.payload ?? {}),
        card: { kind: `event`, event: `trigger_fired`, agentId: agent.id, triggerId: trigger.id, eventId, name },
        sealed: false,
        waking: true,
      })
      ctx.effects.push({ kind: `nudge`, deviceId: conversation.deviceId })
    }
  }
  return {}
}

// ── the reducer ──────────────────────────────────────────────────────────────

/** One event → the next state, the effects to apply and the step's result (or an eCore refusal, state unchanged). */
export function reduce(state: HubState, event: HubEvent): Outcome {
  const ctx: Ctx = { state: structuredClone(state), effects: [], now: event.at, eventId: event.id }
  const data = (event.data ?? {}) as Record<string, unknown>
  try {
    const caller = () => (data.callerId ? session(ctx.state, data.callerId as string) : null)
    let result: StepResult
    switch (event.kind) {
      case `send.start`:
        result = data.parentId ? startChild(ctx, data as SendData, caller()) : startRoot(ctx, data as SendData, caller())
        break
      case `send.message`:
        result = sendMessage(ctx, data as SendData, caller())
        break
      case `send.watch`:
        result = { sessionId: data.watcherId as string }
        watchSessions(ctx, data.watcherId as string, (data.ids as string[]) ?? [])
        break
      case `session.end`: {
        const top = session(ctx.state, data.sessionId as string)
        // #FEED-192: ending never needs an encrypted body: a plaintext summary into a keyed tree is dropped, never stored.
        const plain = (data.summary as string | undefined)?.trim() && data.summarySealed !== true
        const dropped = plain && ctx.state.sessions[top.rootId]?.sealEpoch != null
        const endedIds = endRows(ctx, liveSubtree(ctx.state, top), {
          id: top.id,
          summary: dropped ? null : ((data.summary as string | undefined) ?? null),
          summarySealed: data.summarySealed === true,
        })
        result = { sessionId: top.id, endedIds }
        break
      }
      case `session.pr_opened`: {
        const opener = session(ctx.state, data.sessionId as string)
        busLine(ctx, opener, `pr_opened`, prOpenedText(data.number as number, data.url as string), {
          waking: true,
          pullRequestId: data.pullRequestId as string,
        })
        result = { sessionId: opener.id }
        break
      }
      case `device.claim`:
        result = claim(ctx, data)
        break
      case `device.report`:
        result = report(ctx, data as ReportData)
        break
      case `device.reply`:
        result = reply(ctx, data as ReplyData)
        break
      case `device.consume`:
        result = consume(ctx, data)
        break
      case `device.subagent`:
        result = subagentEdge(ctx, data as SubagentData)
        break
      case `device.heartbeat`:
        result = heartbeat(ctx, data as BeatData)
        break
      case `watchdog.tick`:
        result = tick(ctx)
        break
      case `trigger.schedule`:
        result = schedule(ctx, data as ScheduleData)
        break
      case `trigger.event`:
        result = emitted(ctx, data as EmittedData, event.id)
        break
      default:
        return { state, effects: [], failure: failure(`invalid`, `Unknown event ${event.kind}`) }
    }
    return { state: ctx.state, effects: ctx.effects, result }
  } catch (error) {
    if (error instanceof Refusal) return { state, effects: [], failure: error.failure }
    throw error
  }
}

/** A fixture's `given` with rules.json `defaults` filled in. */
export function hydrate(given: {
  sessions?: Array<Partial<HubSession> & { id: string }>
  messages?: Array<Partial<HubMessage> & { id: string; sessionId: string }>
  devices?: Array<Partial<{ lastSeenAt: number; liveSessionIds: string[] | null }> & { id: string }>
  agents?: Array<Partial<import("./model").HubAgent> & { id: string; ownerId: string; rootSessionId: string }>
  /** #EXP-1735: teamId → its lead Assistant. */
  leads?: Record<string, string>
}): HubState {
  const d = rules.defaults
  const state: HubState = { seq: 0, sessions: {}, messages: [], devices: {}, agents: {}, leads: { ...(given.leads ?? {}) } }
  const rows = given.sessions ?? []
  const byId = new Map(rows.map((row) => [row.id, row]))
  const place = (row: Partial<HubSession> & { id: string }): { rootId: string; depth: number } => {
    if (!row.parentId) return { rootId: row.id, depth: 0 }
    const parent = byId.get(row.parentId)
    if (!parent) throw new Error(`given: ${row.id}'s parent ${row.parentId} is missing`)
    const up = place(parent)
    return { rootId: up.rootId, depth: up.depth + 1 }
  }
  for (const row of rows) {
    const placed = place(row)
    // A child shares its root's visibility unless the case says otherwise.
    const shared = row.shared ?? byId.get(placed.rootId)?.shared ?? d.session.shared
    state.sessions[row.id] = { ...(d.session as Omit<HubSession, `id` | `rootId` | `depth`>), ...placed, shared, ...row } as HubSession
    state.sessions[row.id]!.wakeOn = [...(row.wakeOn ?? [])]
  }
  for (const m of given.messages ?? []) state.messages.push({ ...(d.message as Omit<HubMessage, `id` | `sessionId`>), ...m } as HubMessage)
  for (const device of given.devices ?? []) state.devices[device.id] = { ...d.device, ...device }
  for (const agent of given.agents ?? []) {
    state.agents[agent.id] = { ...d.agent, teamId: d.session.teamId, ...agent, firedAt: { ...(agent.firedAt ?? {}) } } as import("./model").HubAgent
  }
  return state
}
