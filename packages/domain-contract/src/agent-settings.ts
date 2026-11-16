// EXP-1291/NEW-2: Team settings → Agents. Pure and byte-locked by
// `fixtures/agent-settings.json` (`summaryCases`, `validationCases`), which
// every mirror replays: desktop `domain::agent_settings`, iOS ExpCore
// `AgentSettings.swift`, Android `domain/AgentSettings.kt`.

import fixture from "../fixtures/agent-settings.json" with { type: "json" }
import contractJson from "../contract.json" with { type: "json" }
// #EXP-1859: the wake rules live in eTalk's fixtures (JSON only: @exp/etalk
// depends on this package).
import voiceFixture from "../../etalk/fixtures/voice.json" with { type: "json" }
import talkListenFixture from "../../etalk/fixtures/talk-listen.json" with { type: "json" }
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

// #EXP-1859 (`voice.wake`): WAKE PHRASES PER AGENT. The member's voice
// device listens for `wakeListeners`; the team agent page's `wake-word` row
// reads `wakeRow` and flips through `wakeToggle` (tRPC `voice.wakeAgent`);
// `agents.update` refuses a phrase `wakePhraseClash` names. Letters and the
// edit tolerance are talk-listen.json `wake.words` / `maxEditRatio`, the
// minimum voice.json `limits.wakePhraseMinLetters` (`wakePhraseError`).
export const AGENT_VOICE_WAKE = fixture.voice.wake
export const AGENT_VOICE_WAKE_COPY = fixture.voice.wake.copy
export const WAKE_MAX_LISTENERS = fixture.voice.wake.maxListeners
export const WAKE_MAX_IDS = fixture.voice.wake.maxIds
/** talk-listen.json `wake.constants.maxEditRatio`. */
export const WAKE_MAX_EDIT_RATIO = talkListenFixture.wake.constants.maxEditRatio
/** voice.json `limits.wakePhraseMinLetters`. */
export const WAKE_PHRASE_MIN_LETTERS = voiceFixture.limits.wakePhraseMinLetters

export interface WakeAgentLike {
  id: string
  role?: string | null
  name: string
  voice?: AgentVoiceLike | null
}

export interface WakeSettingsLike extends MemberVoiceLike {
  voiceDeviceId: string | null
  /** The voice agent (the member's Assistant). */
  agentId: string | null
  wakeWord: boolean
  wakeAgentIds: readonly string[]
}

export interface WakeInput {
  settings: WakeSettingsLike
  /** The agents of the member's teams, as synced. */
  agents: readonly WakeAgentLike[]
}

export interface WakeListener {
  agentId: string
  phrase: string
  language: string
}

// The same table as the desktop's `wake_words` (etalk-engine wake.rs).
const WAKE_FOLD: Record<string, string> = {
  ä: `a`, á: `a`, à: `a`, â: `a`, å: `a`,
  ö: `o`, ó: `o`, ò: `o`, ô: `o`,
  ü: `u`, ú: `u`, ù: `u`, û: `u`,
  é: `e`, è: `e`, ê: `e`, ë: `e`,
  í: `i`, ì: `i`, î: `i`, ï: `i`,
  ç: `c`, ñ: `n`, ß: `ss`,
}

/** A phrase's letters (talk-listen.json `wake.words` joined without
 *  spaces): letters and digits, lower case, accents folded. */
export function wakeLetters(phrase: string): string {
  let out = ``
  for (const char of phrase) {
    if (!/[\p{L}\p{N}]/u.test(char)) continue
    for (const lower of char.toLowerCase()) out += WAKE_FOLD[lower] ?? lower
  }
  return out
}

function levenshtein(a: readonly string[], b: readonly string[]): number {
  const row = Array.from({ length: b.length + 1 }, (_, index) => index)
  for (let i = 0; i < a.length; i++) {
    let diagonal = row[0]!
    row[0] = i + 1
    for (let j = 0; j < b.length; j++) {
      const above = row[j + 1]!
      row[j + 1] = Math.min(above + 1, row[j]! + 1, diagonal + (a[i] === b[j] ? 0 : 1))
      diagonal = above
    }
  }
  return row[b.length]!
}

/** `clashCases`: two phrases alike enough for the matcher to take one for
 *  the other, or one starting with the other. */
export function wakeClash(a: string, b: string): boolean {
  const left = [...wakeLetters(a)]
  const right = [...wakeLetters(b)]
  const [short, long] = right.length < left.length ? [right, left] : [left, right]
  const tolerance = Math.floor(short.length * WAKE_MAX_EDIT_RATIO)
  return Math.min(levenshtein(short, long), levenshtein(short, long.slice(0, short.length))) <= tolerance
}

/** The phrase an agent wakes with: its voice's (agentVoiceOf), else
 *  `Hey {name}`. */
export function wakePhraseOf(agent: WakeAgentLike, member: MemberVoiceLike): string {
  const phrase = agentVoiceOf(agent, member).wakePhrase
  return phrase.trim() ? phrase : voiceFixture.copy.wakePhrasePlaceholder.replace(`{agent}`, agent.name)
}

/** voice.json `wakePhraseError` for a resolved phrase: too few letters. */
function wakeTooShort(phrase: string): boolean {
  return [...phrase.trim()].filter((char) => /\p{L}/u.test(char)).length < WAKE_PHRASE_MIN_LETTERS
}

const isTeamAgent = (agent: { role?: string | null }) => agent.role !== `assistant`

type WakeRefusal = { kind: `tooShort` } | { kind: `clash`; agent: string } | { kind: `cap` }

function refusalText(refusal: WakeRefusal | null): string | null {
  if (!refusal) return null
  const errors = AGENT_VOICE_WAKE_COPY.errors
  if (refusal.kind === `tooShort`) return errors.wakeTooShort
  if (refusal.kind === `clash`) return errors.wakeClash.replace(`{agent}`, refusal.agent)
  return errors.wakeCap
}

/** Why `agent` would not listen beside `listeners`: too short, a clash
 *  (the first listener it sounds like), the cap; null = it listens. */
function wakeRefusal(
  agent: WakeAgentLike,
  input: WakeInput,
  listeners: readonly (WakeListener & { name: string })[]
): WakeRefusal | null {
  const phrase = wakePhraseOf(agent, input.settings)
  if (wakeTooShort(phrase)) return { kind: `tooShort` }
  const clash = listeners.find((listener) => wakeClash(listener.phrase, phrase))
  if (clash) return { kind: `clash`, agent: clash.name }
  if (listeners.length >= WAKE_MAX_LISTENERS) return { kind: `cap` }
  return null
}

function namedListeners(input: WakeInput): (WakeListener & { name: string })[] {
  const { settings, agents } = input
  if (!settings.wakeWord || !settings.voiceDeviceId || !settings.agentId) return []
  const voiceAgent = agents.find((agent) => agent.id === settings.agentId)
  if (!voiceAgent) return []
  const out: (WakeListener & { name: string })[] = [
    {
      agentId: voiceAgent.id,
      phrase: wakePhraseOf(voiceAgent, settings),
      language: agentVoiceOf(voiceAgent, settings).language,
      name: voiceAgent.name,
    },
  ]
  for (const id of settings.wakeAgentIds) {
    const agent = agents.find((candidate) => candidate.id === id)
    if (!agent || !isTeamAgent(agent) || out.some((listener) => listener.agentId === id)) continue
    if (wakeRefusal(agent, input, out)) continue
    out.push({
      agentId: agent.id,
      phrase: wakePhraseOf(agent, settings),
      language: agentVoiceOf(agent, settings).language,
      name: agent.name,
    })
  }
  return out
}

/** `listenersCases`: who the voice device listens for, in order: the voice
 *  agent, then the switched-on team agents that fit (at most 5). */
export function wakeListeners(input: WakeInput): WakeListener[] {
  return namedListeners(input).map(({ agentId, phrase, language }) => ({ agentId, phrase, language }))
}

/** `rowCases`: a team agent's Wake word row: on, and why it does not listen
 *  (null = it does, or it is off). */
export function wakeRow(input: WakeInput, agentId: string): { on: boolean; error: string | null } {
  const { settings } = input
  const on = settings.wakeWord && settings.wakeAgentIds.includes(agentId)
  if (!on) return { on, error: null }
  const agent = input.agents.find((candidate) => candidate.id === agentId)
  if (!agent || !isTeamAgent(agent)) return { on, error: null }
  const index = settings.wakeAgentIds.indexOf(agentId)
  const before = namedListeners({ ...input, settings: { ...settings, wakeAgentIds: settings.wakeAgentIds.slice(0, index) } })
  if (before.some((listener) => listener.agentId === agentId)) return { on, error: null }
  return { on, error: refusalText(wakeRefusal(agent, input, before)) }
}

/** `toggleCases`: a flip of the row. Off drops the id (Wake word as it
 *  was); on appends it once and turns Wake word on, unless refused (the
 *  settings unchanged, `error` = the copy). An agent that is not a team
 *  agent in `agents` is the caller's NOT_FOUND (never reached here). */
export function wakeToggle(
  input: WakeInput,
  agentId: string,
  on: boolean
): { wakeWord: boolean; wakeAgentIds: string[]; error: string | null } {
  const { settings } = input
  const ids = [...settings.wakeAgentIds]
  if (!on) return { wakeWord: settings.wakeWord, wakeAgentIds: ids.filter((id) => id !== agentId), error: null }
  const unchanged = (error: string) => ({ wakeWord: settings.wakeWord, wakeAgentIds: ids, error })
  const agent = input.agents.find((candidate) => candidate.id === agentId)
  // Not a team agent of the member's teams: the server's NOT_FOUND first.
  if (!agent || !isTeamAgent(agent)) return { wakeWord: settings.wakeWord, wakeAgentIds: ids, error: null }
  const others = ids.filter((id) => id !== agentId)
  const listeners = namedListeners({ ...input, settings: { ...settings, wakeWord: true, wakeAgentIds: others } }).filter(
    (listener) => listener.agentId !== agentId
  )
  const error = refusalText(wakeRefusal(agent, input, listeners))
  if (error) return unchanged(error)
  return { wakeWord: true, wakeAgentIds: ids.includes(agentId) ? ids : [...ids, agentId], error: null }
}

/** `phraseClashCases`: the first OTHER team agent (an Assistant never
 *  counts) whose phrase `phrase` sounds like; null = none. */
export function wakePhraseClash(phrase: string, agentId: string, agents: readonly WakeAgentLike[]): string | null {
  // A team agent's phrase is its own or `Hey {name}`: no member field counts.
  const member: MemberVoiceLike = { language: ``, voiceFile: ``, voiceTranscript: ``, wakePhrase: `` }
  const clash = agents.find(
    (agent) => agent.id !== agentId && isTeamAgent(agent) && wakeClash(wakePhraseOf(agent, member), phrase)
  )
  return clash?.name ?? null
}
