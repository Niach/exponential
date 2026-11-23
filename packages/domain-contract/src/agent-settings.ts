// EXP-1291/NEW-2: Team settings → Agents. Pure and byte-locked by
// `fixtures/agent-settings.json` (`summaryCases`, `validationCases`), which
// every mirror replays: desktop `domain::agent_settings`, iOS ExpCore
// `AgentSettings.swift`, Android `domain/AgentSettings.kt`.

import fixture from "../fixtures/agent-settings.json" with { type: "json" }
import contractJson from "../contract.json" with { type: "json" }
import { scheduleSummary } from "./schedule-picker"

export const AGENT_SETTINGS_FIELDS = fixture.fields
export const AGENT_SETTINGS_COPY = fixture.copy

export interface AgentTriggerLike {
  kind: string
  enabled?: boolean
  /** schedule: the 5-field cron and its IANA zone. */
  cron?: string
  tz?: string | null
  /** event: the events row name it listens on. */
  name?: string
  /** #EXP-1846: the Check in preset this schedule is (`checkIn`). */
  preset?: string | null
  /** #EXP-1850: record: the record kind and the op it listens on. */
  record?: string
  op?: string
  /** #EXP-1850: an app binding the trigger runs instead of a wake. */
  run?: Record<string, unknown> | null
}

/** One Triggers row's text. */
export function triggerSummary(trigger: AgentTriggerLike): string {
  const copy = fixture.copy
  let text = ``
  if (trigger.kind === `schedule`) {
    // NEW-2: the schedule picker's sentence (`schedule-picker.json`).
    text = scheduleSummary(trigger.cron ?? ``, trigger.tz)
  } else if (trigger.kind === `record`) {
    // #EXP-1850: an app agent's record trigger.
    const op = (copy.recordOps as Record<string, string>)[trigger.op ?? ``] ?? trigger.op ?? ``
    text = copy.record.replace(`{record}`, trigger.record ?? ``).replace(`{op}`, op)
  } else if (trigger.kind === `webhook`) {
    text = copy.webhook.replace(`{name}`, trigger.name ?? ``)
  } else {
    const name = trigger.name ?? ``
    text = (copy.events as Record<string, string>)[name] ?? copy.event.replace(`{name}`, name)
  }
  return trigger.enabled === false ? `${text} · ${copy.paused}` : text
}

/** #EXP-1850: an app binding in a few words (`detailCases`). */
export function runLabel(run: Record<string, unknown>): string {
  const records = run.records as { op?: string; kind?: string } | undefined
  if (records && typeof records === `object`) return `records.${records.op ?? ``} ${records.kind ?? ``}`
  if (typeof run.action === `string`) return run.action
  if (typeof run.upstream === `string`) return `${run.upstream}/${typeof run.tool === `string` ? run.tool : ``}`
  return ``
}

/** #EXP-1850: a Triggers row's second line: what a `run` trigger runs; null
 *  for a wake (`detailCases`). */
export function triggerDetail(trigger: AgentTriggerLike): string | null {
  const run = trigger.run
  if (!run || typeof run !== `object`) return null
  if (typeof run.agent === `string`) return fixture.copy.handsOff.replace(`{agent}`, run.agent)
  return fixture.copy.runs.replace(`{tool}`, runLabel(run))
}

export interface AgentForm {
  name: string
  deviceId: string | null
  instructions: string
}

export interface AgentFormError {
  field: string
  message: string
}

/** Field errors in field order; [] = valid. */
export function validateAgentForm(form: AgentForm): AgentFormError[] {
  const errors: AgentFormError[] = []
  const caps = contractJson.sessionsSend
  const name = form.name.trim()
  if (!name) errors.push({ field: `identity`, message: fixture.copy.errors.nameRequired })
  else if ([...name].length > caps.agentNameMax)
    errors.push({ field: `identity`, message: fixture.copy.errors.nameTooLong })
  if (!form.deviceId) errors.push({ field: `device`, message: fixture.copy.errors.deviceRequired })
  if (new TextEncoder().encode(form.instructions).length > caps.agentInstructionsMaxBytes)
    errors.push({ field: `instructions`, message: fixture.copy.errors.instructionsTooLong })
  return errors
}

export interface AgentAccountOptionLike {
  id: string
  agent: string
  email: string
  isLastUsed: boolean
}

/** EXP-1306: the login the Account row reads (its agent mark + email, the
 *  composer chip's value); null = no login reported (`copy.accountLastUsed`). */
export function agentAccountValue<T extends AgentAccountOptionLike>(input: {
  options: readonly T[]
  account: string | null
  cli: string | null
}): T | null {
  const { options, account, cli } = input
  return (
    (account ? options.find((option) => option.id === account && (!cli || option.agent === cli)) : undefined) ??
    (cli ? options.find((option) => option.agent === cli) : undefined) ??
    options.find((option) => option.isLastUsed) ??
    options[0] ??
    null
  )
}

export interface AgentActorRowLike {
  id: string
  name: string
  icon: string | null
  color: string | null
}

export type IssueActor =
  | { kind: `agent`; id: string; name: string; icon: string | null; color: string | null }
  | { kind: `user`; id: string }

/** #EXP-1383: who an issue event / comment / issue creation reads as
 *  (`actor.actorCases`): a synced agent first, else the member, else null. */
export function issueActor(
  input: { actorAgentId: string | null | undefined; actorUserId: string | null | undefined },
  agents: readonly AgentActorRowLike[]
): IssueActor | null {
  const agent = input.actorAgentId ? agents.find((row) => row.id === input.actorAgentId) : undefined
  if (agent) return { kind: `agent`, id: agent.id, name: agent.name, icon: agent.icon, color: agent.color }
  if (input.actorUserId) return { kind: `user`, id: input.actorUserId }
  return null
}

/** #EXP-1383: the assignee picker's option ids ×4 (`assignee.optionCases`):
 *  unassigned, the members in the given order, then the team's agents by
 *  sortOrder then name. */
export function assigneeOptions(input: {
  members: readonly { id: string }[]
  agents: readonly { id: string; name: string; sortOrder: number }[]
}): string[] {
  const agents = [...input.agents].sort(
    (a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name) || a.id.localeCompare(b.id)
  )
  return [
    `unassigned`,
    ...input.members.map((member) => `user:${member.id}`),
    ...agents.map((agent) => `agent:${agent.id}`),
  ]
}

// S-3 (owner 2026-10-28): THE ASSISTANT (`assistant` in the fixture; contract
// `agentRole`). Replayed by every mirror (`scheduleCases`, `visibleCases`,
// `orderCases`).
export const ASSISTANT = fixture.assistant
export const ASSISTANT_COPY = fixture.assistant.copy
export type AssistantScheduleId = `off` | `15m` | `30m` | `1h` | `daily`
export const ASSISTANT_SCHEDULES = fixture.assistant.schedules as {
  id: AssistantScheduleId
  label: string
  cron: string | null
}[]

/** The preset stored triggers stand for; null = not an Assistant's
 *  (an event, a custom cron, more than one) and `agents.update` refuses it. */
export function assistantScheduleOf(triggers: AgentTriggerLike[]): AssistantScheduleId | null {
  if (triggers.length === 0) return `off`
  if (triggers.length > 1) return null
  const [trigger] = triggers
  if (trigger!.kind !== `schedule`) return null
  const preset = ASSISTANT_SCHEDULES.find((schedule) => schedule.cron !== null && schedule.cron === trigger!.cron)
  if (!preset) return null
  return trigger!.enabled === false ? `off` : preset.id
}

/** The triggers a preset saves as (`off` = none), tagged as the Check in
 *  (#EXP-1846). */
export function assistantScheduleTriggers(
  id: AssistantScheduleId,
  tz?: string | null
): { kind: `schedule`; cron: string; preset: Exclude<AssistantScheduleId, `off`>; enabled: true; tz?: string }[] {
  const cron = ASSISTANT_SCHEDULES.find((schedule) => schedule.id === id)?.cron
  if (!cron || id === `off`) return []
  return [{ kind: `schedule`, cron, preset: id, enabled: true, ...(tz ? { tz } : {}) }]
}

/** The Assistant's Model row value (`modelValueCases`): the pinned model,
 *  else the device default for the login's CLI (never `Last
 *  used`). */
export function assistantModelValue(input: { model?: string | null; cli?: string | null }): string {
  const pinned = input.model?.trim()
  if (pinned) return input.model!
  const defaults = contractJson.deviceAgentDefaults
  return input.cli === `codex` ? defaults.codexModel : defaults.model
}

/** NEW-99 (`runsOnCases`): the Runs on device picker's devices. An
 *  Assistant's device is the member's voice device, so it lists desktops
 *  only (a bound non-desktop stays, naming itself); a team agent keeps all. */
export function runsOnDeviceIds(
  devices: readonly { deviceId: string; kind?: string | null }[],
  boundId: string | null,
  assistant: boolean
): string[] {
  return devices
    .filter((device) => !assistant || device.kind === `desktop` || device.deviceId === boundId)
    .map((device) => device.deviceId)
}

export interface AgentRoleLike {
  role?: string | null
  ownerId: string
}

/** Another member's Assistant is never listed (settings, composer, chats). */
export function visibleAgent(agent: AgentRoleLike, me: string | null | undefined): boolean {
  return agent.role !== `assistant` || agent.ownerId === me
}

export function isAssistant(agent: { role?: string | null }): boolean {
  return agent.role === `assistant`
}

// #EXP-1837 (`assistant.fab`): THE Assistant FAB + widget on the IDE and
// wide web (phones keep the Assistant in Chats).
export const ASSISTANT_FAB = fixture.assistant.fab
export const ASSISTANT_FAB_COPY = fixture.assistant.fab.copy
export const ASSISTANT_WIDGET = fixture.assistant.fab.widget

export type WidgetSize = { width: number; height: number }

/** #EXP-1876 `widget.sizeCases`: the widget's size in a content card of
 *  `card` px. No `stored` = the responsive default (round(card × ratio)
 *  clamped to [min … max]); a dragged `stored` size clamps to [min … card −
 *  margin]. Either never exceeds card − margin (the room beats the floor). */
export function assistantWidgetSize(input: { card: WidgetSize; stored?: WidgetSize | null }): WidgetSize {
  const axis = (key: keyof WidgetSize) => {
    const { min, ratio, max, margin } = ASSISTANT_WIDGET
    const room = Math.max(0, input.card[key] - margin[key])
    const stored = input.stored?.[key]
    const wanted =
      stored != null && Number.isFinite(stored)
        ? Math.max(Math.round(stored), min[key])
        : Math.min(Math.max(Math.round(input.card[key] * ratio[key]), min[key]), max[key])
    return Math.min(wanted, room)
  }
  return { width: axis(`width`), height: axis(`height`) }
}

/** `showCases`: the FAB shows on wide surfaces while the viewer has an
 *  Assistant, except on its own chat page. */
export function assistantFabVisible(input: {
  wide: boolean
  assistantId: string | null | undefined
  activeAgentId: string | null | undefined
}): boolean {
  return input.wide && Boolean(input.assistantId) && input.activeAgentId !== input.assistantId
}

// #EXP-1876 (`assistant.fab.entrance` / `bubble`): the 60px mark-button's
// first-show entrance and THE speech bubble above it.
export const ASSISTANT_FAB_ENTRANCE = fixture.assistant.fab.entrance
export const ASSISTANT_FAB_BUBBLE = fixture.assistant.fab.bubble

/** A waiting card of the Assistant conversation's tree (an unanswered plan /
 *  question / ask card): what turns the FAB's badge yellow. */
export interface AssistantFabWaitingCard {
  messageId: string
  sessionId: string
  createdAt: string
}

export type AssistantFabBubble = { kind: `welcome` } | { kind: `needsYou`; messageId: string } | null

/** `bubble.cases`: which speech bubble the FAB shows now. Nothing while the
 *  widget is open, on the Assistant's page or while the window is unfocused;
 *  else the newest waiting card this device has not bubbled and the
 *  viewer's reads do not cover; else the welcome on the device's first
 *  sight; else nothing (a plain new message = the badge only). */
export function assistantFabBubble(input: {
  welcomeSeen: boolean
  cards: readonly AssistantFabWaitingCard[]
  reads: readonly { sessionId: string; readAt: string }[]
  bubbled: readonly string[]
  widgetOpen: boolean
  onAssistantPage: boolean
  focused: boolean
}): AssistantFabBubble {
  if (input.widgetOpen || input.onAssistantPage || !input.focused) return null
  const at = (iso: string) => Date.parse(iso)
  const bubbled = new Set(input.bubbled)
  const covered = (card: AssistantFabWaitingCard) =>
    input.reads.some((read) => read.sessionId === card.sessionId && at(read.readAt) >= at(card.createdAt))
  const newest = input.cards
    .filter((card) => !bubbled.has(card.messageId) && !covered(card))
    .sort((a, b) => at(b.createdAt) - at(a.createdAt) || (a.messageId < b.messageId ? 1 : -1))[0]
  if (newest) return { kind: `needsYou`, messageId: newest.messageId }
  return input.welcomeSeen ? null : { kind: `welcome` }
}

/** The agents a member sees, in order: their Assistant first, then the team
 *  agents by sortOrder, then name. */
export function orderedAgents<T extends AgentRoleLike & { name: string; sortOrder: number }>(
  agents: T[],
  me: string | null | undefined
): T[] {
  const visible = agents.filter((agent) => visibleAgent(agent, me))
  return [...visible].sort((a, b) => {
    const rank = Number(!isAssistant(a)) - Number(!isAssistant(b))
    if (rank !== 0) return rank
    if (a.sortOrder !== b.sortOrder) return a.sortOrder - b.sortOrder
    return a.name.localeCompare(b.name)
  })
}

export interface TeamAgentLike {
  id: string
  teamId: string
  role?: string | null
  ownerId: string
}

/** #EXP-1714 (`assistant.teamAgentsCases`): the agents a team lists: its
 *  own rows (input order), plus the viewer's global Assistant (its team is
 *  none of `teamIds`, the synced teams: a personal team never syncs) while
 *  the team holds no Assistant of theirs. `visibleAgent` still applies. */
export function agentsInTeam<T extends TeamAgentLike>(
  agents: readonly T[],
  teamId: string,
  teamIds: readonly string[],
  me: string | null | undefined
): T[] {
  const own = agents.filter((agent) => agent.teamId === teamId)
  if (own.some((agent) => isAssistant(agent) && agent.ownerId === me)) return own
  const global = agents.find(
    (agent) => isAssistant(agent) && agent.ownerId === me && !teamIds.includes(agent.teamId)
  )
  return global ? [global, ...own] : own
}

/** #EXP-1988 (`assistant.teamPicks`): the picks for `teamId` in a flat skill
 *  / MCP server list = the listed ids whose row (`rowTeams`: row id → its
 *  team) is that team's, in pick order. A gone row belongs to no team. */
export function teamPick(ids: readonly string[], rowTeams: Readonly<Record<string, string>>, teamId: string): string[] {
  return [...new Set(ids)].filter((id) => rowTeams[id] === teamId)
}

/** #EXP-1988 (`teamPicks.mergeCases`): an `agents.update` write of a skill /
 *  MCP list. With `pickTeamId` it replaces only that team's picks; without
 *  it (old clients, MCP, team agents) the picks of the agent's own team and
 *  of every team the written ids name. Gone rows drop out. */
export function mergeTeamPicks(input: {
  existing: readonly string[]
  written: readonly string[]
  rowTeams: Readonly<Record<string, string>>
  pickTeamId: string | null
  agentTeamId: string
}): string[] {
  const { rowTeams } = input
  const written = [...new Set(input.written)]
  const replaced = new Set(
    input.pickTeamId
      ? [input.pickTeamId]
      : [input.agentTeamId, ...written.flatMap((id) => (rowTeams[id] ? [rowTeams[id]!] : []))]
  )
  const kept = [...new Set(input.existing)].filter((id) => rowTeams[id] !== undefined && !replaced.has(rowTeams[id]!))
  return [...kept, ...written.filter((id) => !kept.includes(id))]
}

/** #EXP-1988 (`teamPicks.launchCases`): what a session launches with = its
 *  list ∩ the rows of its launch teams; a gone row stays (it is skipped
 *  with a reason at launch). */
export function launchPick(
  ids: readonly string[],
  rowTeams: Readonly<Record<string, string>>,
  launchTeams: readonly string[]
): string[] {
  return [...new Set(ids)].filter((id) => rowTeams[id] === undefined || launchTeams.includes(rowTeams[id]!))
}

/** #EXP-1988 (`teamPicks.pickAgentCases`): the agent a team's section of
 *  Settings › Assistant edits: the viewer's Assistant `agentsInTeam` lists
 *  in that team, else the page's Assistant. */
export function assistantPickAgent<T extends TeamAgentLike>(
  agents: readonly T[],
  teamId: string,
  teamIds: readonly string[],
  me: string | null | undefined,
  pageAgentId: string | null
): string | null {
  const listed = agentsInTeam(agents, teamId, teamIds, me).find((agent) => isAssistant(agent) && agent.ownerId === me)
  return listed?.id ?? pageAgentId
}

export const ASSISTANT_TEAM_PICKS = fixture.assistant.teamPicks

export const ASSISTANT_MOVE = fixture.assistant.move
/** #EXP-1984 (`move.clientFloor`): the oldest build per platform that lists
 *  the global Assistant in every team. */
export const ASSISTANT_CLIENT_FLOOR = fixture.assistant.move.clientFloor as Record<`desktop` | `android` | `ios`, string>

export interface MoveAssistantLike {
  id: string
  ownerId: string
  createdAt: string
}
export interface MoveRootLike {
  id: string
  agentId: string
  lastActivityAt: string | null
  createdAt: string
}
export interface AssistantMove {
  ownerId: string
  keepAgentId: string
  keepRootId: string | null
  endedRootIds: string[]
  dropAgentIds: string[]
}

/** #EXP-1984 (`move.keepCases`): per owner (in owner order), the Assistant
 *  and conversation root the move keeps, the roots it ends and the rows it
 *  drops. The kept root = the latest activity (else creation), ties to the
 *  lower id; no root at all = the newest Assistant row. */
export function assistantMoveOf(assistants: readonly MoveAssistantLike[], roots: readonly MoveRootLike[]): AssistantMove[] {
  const cmpId = (a: { id: string }, b: { id: string }) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
  const at = (iso: string) => Date.parse(iso)
  const owners = [...new Set(assistants.map((agent) => agent.ownerId))].sort()
  return owners.map((ownerId) => {
    const mine = assistants.filter((agent) => agent.ownerId === ownerId)
    const ids = new Set(mine.map((agent) => agent.id))
    const theirRoots = roots
      .filter((root) => ids.has(root.agentId))
      .sort((a, b) => at(b.lastActivityAt ?? b.createdAt) - at(a.lastActivityAt ?? a.createdAt) || cmpId(a, b))
    const keepRoot = theirRoots[0] ?? null
    const keepAgentId =
      keepRoot?.agentId ?? [...mine].sort((a, b) => at(b.createdAt) - at(a.createdAt) || cmpId(a, b))[0]!.id
    return {
      ownerId,
      keepAgentId,
      keepRootId: keepRoot?.id ?? null,
      endedRootIds: theirRoots.slice(1).map((root) => root.id).sort(),
      dropAgentIds: mine.map((agent) => agent.id).filter((id) => id !== keepAgentId).sort(),
    }
  })
}

export const GROUP_CHAT = fixture.groupChat
/** #EXP-1990 (`groupChat.clientFloor`): the oldest build per platform that
 *  opens a team agent's chat by `agents.groupChatId`. */
export const GROUP_CHAT_CLIENT_FLOOR = fixture.groupChat.clientFloor as Record<`desktop` | `android` | `ios`, string>

export interface AgentChatSessionLike {
  id: string
  parentId: string | null
  agentId?: string | null
  userId: string
}

/** #EXP-1990 (`groupChat.chatCases`): THE chat of an agent for the viewer:
 *  its group chat (`groupChatId`) when that root is synced, none when it
 *  names a root the viewer does not sync; with no group chat the root with
 *  its `agentId` the viewer started (the Assistant; a team agent before the
 *  switch). */
export function agentChatOf<T extends AgentChatSessionLike>(input: {
  agent: { agentId: string; groupChatId?: string | null }
  sessions: readonly T[]
  me: string | null | undefined
}): T | null {
  const { agent, sessions, me } = input
  if (agent.groupChatId) {
    return sessions.find((session) => session.id === agent.groupChatId && session.parentId === null) ?? null
  }
  if (!me) return null
  return (
    sessions.find((session) => session.parentId === null && session.agentId === agent.agentId && session.userId === me) ??
    null
  )
}

/** #EXP-1990 (`groupChat.moveCases`): the conversation roots the move ends,
 *  detaches and turns private: every root of a team agent (never an
 *  Assistant's, never an app's editor root) that is not its agent's group
 *  chat; by id. */
export function groupChatMoveOf(
  agents: readonly { id: string; role?: string | null; groupChatId?: string | null }[],
  roots: readonly { id: string; agentId: string; appId?: string | null }[]
): string[] {
  const byId = new Map(agents.map((agent) => [agent.id, agent]))
  return roots
    .filter((root) => {
      const agent = byId.get(root.agentId)
      return !!agent && !isAssistant(agent) && !root.appId && agent.groupChatId !== root.id
    })
    .map((root) => root.id)
    .sort()
}

/** #EXP-1990 (`groupChat.askCases`): who is pushed a question or plan a
 *  team agent asks in its group chat: the asker (the member whose message
 *  the agent is working on) while a member, else the agent's owner while a
 *  member, else nobody. */
export function groupAskRecipient(input: {
  turnStarterId: string | null
  ownerId: string
  memberIds: readonly string[]
}): string[] {
  if (input.turnStarterId && input.memberIds.includes(input.turnStarterId)) return [input.turnStarterId]
  return input.memberIds.includes(input.ownerId) ? [input.ownerId] : []
}

export const COORDINATOR = fixture.coordinator
export const COORDINATOR_COPY = fixture.coordinator.copy
/** The option id of "no coordinator". */
export const COORDINATOR_NONE = `none`

export function isCoordinator(agent: { role?: string | null }): boolean {
  return agent.role === `coordinator`
}

/** #EXP-1717 (`coordinator.optionCases`): the Coordinator row's options:
 *  `none`, then the team agents (never an Assistant) by sortOrder, then name. */
export function coordinatorOptions<T extends { id: string; name: string; role?: string | null; sortOrder: number }>(
  agents: readonly T[]
): (T | typeof COORDINATOR_NONE)[] {
  const team = agents
    .filter((agent) => !isAssistant(agent))
    .sort((a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name))
  return [COORDINATOR_NONE, ...team]
}

/** #EXP-1717 (`coordinator.valueCases`): the team's coordinator, or null. */
export function coordinatorOf(
  agents: readonly { id: string; teamId: string; role?: string | null }[],
  teamId: string
): string | null {
  return agents.find((agent) => agent.teamId === teamId && isCoordinator(agent))?.id ?? null
}

export const LEAD = fixture.lead
export const LEAD_COPY = fixture.lead.copy
/** The option id of "no lead Assistant". */
export const LEAD_NONE = `none`

/** #EXP-1735 (`lead.optionCases`): the Lead Assistant row's options:
 *  `none`, then the team's members by display name, then id. */
export function leadOptions<T extends { userId: string; name: string }>(members: readonly T[]): (T | typeof LEAD_NONE)[] {
  const sorted = [...members].sort(
    (a, b) => a.name.localeCompare(b.name) || (a.userId < b.userId ? -1 : a.userId > b.userId ? 1 : 0)
  )
  return [LEAD_NONE, ...sorted]
}

/** #EXP-1793: the agent page's MCP servers row (`mcpRow`). */
export const AGENT_MCP_ROW = fixture.mcpRow

/** The MCP servers row's value: the picked servers' names (team list order)
 *  through the shared picker summary (`a` / `a, b` / `a, b +N`), `None`
 *  when empty (`mcpValueCases`). */
export function agentMcpServersValue(names: readonly string[]): string {
  if (names.length === 0) return fixture.mcpRow.none
  if (names.length <= 2) return names.join(`, `)
  return `${names.slice(0, 2).join(`, `)} +${names.length - 2}`
}

// #EXP-1846 (`page`, `checkIn`): ONE agent page ×4; the Check in is the one
// schedule trigger carrying `preset`, any agent's.
export const AGENT_PAGE = fixture.page
export const CHECK_IN = fixture.checkIn

const presetCron = (id: string) => ASSISTANT_SCHEDULES.find((schedule) => schedule.id === id)?.cron ?? null

/** The Check in row's value (`checkInCases`): the preset trigger's id, `off`
 *  when there is none or it is paused. */
export function checkInOf(triggers: readonly AgentTriggerLike[]): AssistantScheduleId {
  const trigger = triggers.find((candidate) => candidate.kind === `schedule` && candidate.preset)
  if (!trigger || trigger.enabled === false || !presetCron(trigger.preset!)) return `off`
  return trigger.preset as AssistantScheduleId
}

/** The triggers after a Check in pick (`withCheckInCases`): the preset
 *  trigger replaced in front (keeping its id and stored zone) or removed
 *  for `off`; the others unchanged, in order. */
export function withCheckIn<T extends AgentTriggerLike & { id?: string }>(
  triggers: readonly T[],
  id: AssistantScheduleId,
  tz?: string | null
): (T | (AgentTriggerLike & { id?: string }))[] {
  const current = triggers.find((candidate) => candidate.kind === `schedule` && candidate.preset)
  const rest = triggers.filter((candidate) => candidate !== current)
  const cron = presetCron(id)
  if (!cron) return rest
  const zone = current ? current.tz : tz
  const next: AgentTriggerLike & { id?: string } = {
    ...(current?.id ? { id: current.id } : {}),
    kind: `schedule`,
    cron,
    preset: id,
    enabled: true,
    ...(zone ? { tz: zone } : {}),
  }
  return [next, ...rest]
}

/** The rows under Check in (`listedCases`): every trigger without `preset`. */
export function listedTriggers<T extends AgentTriggerLike>(triggers: readonly T[]): T[] {
  return triggers.filter((trigger) => !trigger.preset)
}

/** An OLD client's Assistant Check in write (`legacyCases`): exactly one
 *  untagged schedule on a preset cron = that preset; anything else null. */
export function legacyCheckIn(triggers: readonly AgentTriggerLike[]): AssistantScheduleId | null {
  if (triggers.length !== 1) return null
  const [trigger] = triggers
  if (trigger!.kind !== `schedule` || trigger!.preset) return null
  const preset = ASSISTANT_SCHEDULES.find((schedule) => schedule.cron !== null && schedule.cron === trigger!.cron)
  return preset ? preset.id : null
}

/** The server's rule (`checkIn`): at most one preset trigger, a schedule on
 *  its preset's cron. Null = fine, else the reason. */
export function checkInProblem(triggers: readonly AgentTriggerLike[]): string | null {
  const presets = triggers.filter((trigger) => trigger.preset)
  if (presets.length > 1) return `Only one Check in per agent`
  const [preset] = presets
  if (preset && (preset.kind !== `schedule` || presetCron(preset.preset!) !== preset.cron)) {
    return `The Check in runs on its preset's schedule`
  }
  return null
}

// #EXP-1847 (`voice`): every agent's own voice, resolved against the
// member's voice settings (`voiceOfCases`).
export const AGENT_VOICE = fixture.voice
export const AGENT_VOICE_COPY = fixture.voice.copy

export interface AgentVoiceLike {
  language?: string | null
  clipId?: string | null
  clipName?: string | null
  transcript?: string | null
  wakePhrase?: string | null
}

export interface MemberVoiceLike {
  language: string
  voiceFile: string
  voiceTranscript: string
  wakePhrase: string
}

export interface ResolvedAgentVoice {
  language: string
  clip: { attachmentId: string } | { path: string } | null
  clipLabel: string
  transcript: string
  wakePhrase: string
}

/** The voice an agent speaks with (`voiceOfCases`). */
export function agentVoiceOf(
  agent: { role?: string | null; name: string; voice?: AgentVoiceLike | null },
  member: MemberVoiceLike
): ResolvedAgentVoice {
  const voice = agent.voice ?? {}
  const assistant = agent.role === `assistant`
  const language = voice.language || member.language
  const ownPhrase = voice.wakePhrase?.trim() ? voice.wakePhrase : null
  const wakePhrase = ownPhrase ?? (assistant ? member.wakePhrase : ``)
  if (voice.clipId) {
    return {
      language,
      clip: { attachmentId: voice.clipId },
      clipLabel: voice.clipName || AGENT_VOICE_COPY.clipBuiltIn,
      transcript: voice.transcript ?? ``,
      wakePhrase,
    }
  }
  if (assistant && member.voiceFile) {
    const segments = member.voiceFile.split(/[\\/]/)
    return {
      language,
      clip: { path: member.voiceFile },
      clipLabel: segments[segments.length - 1] || member.voiceFile,
      transcript: voice.transcript ?? member.voiceTranscript,
      wakePhrase,
    }
  }
  return { language, clip: null, clipLabel: AGENT_VOICE_COPY.clipBuiltIn, transcript: voice.transcript ?? ``, wakePhrase }
}
