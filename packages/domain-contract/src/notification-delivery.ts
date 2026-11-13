// #EXP-1815: where a notification shows, when it clears, who mutes it ×4.
// fixtures/notification-delivery.json (its `$comment` is the spec); mirrors:
// desktop `domain::notification_delivery`, iOS ExpPush
// `NotificationDelivery.swift`, Android `domain/NotificationDelivery.kt`.

import fixture from "../fixtures/notification-delivery.json" with { type: "json" }

export const NOTIFICATION_DELIVERY_KEYS = fixture.dataKeys
export const NOTIFICATION_DELIVERY_LIMITS = fixture.limits
export const NOTIFICATION_DELIVERY_COPY = fixture.copy
export const NOTIFICATION_CHANNELS = fixture.channels

// ── Thread ────────────────────────────────────────────────────────────────

export interface ThreadRow {
  type: string
  issueId: string | null
  /** The root of the row's session (a client without it synced: the session id). */
  sessionRootId: string | null
  deviceId: string | null
}

/** The key a notification stacks and clears under. */
export function notificationThread(row: ThreadRow): string {
  if (row.issueId) return `issue:${row.issueId}`
  if (row.sessionRootId) return `session:${row.sessionRootId}`
  if (row.type === `agent_signed_out` && row.deviceId) return `device:${row.deviceId}`
  return `inbox`
}

/** The thread every screen of the Inbox covers. */
export const INBOX_THREAD = `inbox`

// ── Presentation ──────────────────────────────────────────────────────────

export type DeliveryPlatform = `web` | `desktop` | `ios` | `android`
export type Presentation = `none` | `toast` | `system`

export interface PresentationInput {
  platform: DeliveryPlatform
  /** web: the page; desktop: a focused window; phones: the foreground. */
  active: boolean
  /** The current screen's thread (null = a screen without one). */
  viewing: string | null
  thread: string
  /** The source switches muted it (`pushed_at` NULL). */
  muted: boolean
}

export function notificationPresentation(input: PresentationInput): Presentation {
  if (input.muted) return `none`
  if (!input.active) return input.platform === `web` ? `toast` : `system`
  if (input.viewing === INBOX_THREAD || input.viewing === input.thread) return `none`
  return `toast`
}

// ── Clearing ──────────────────────────────────────────────────────────────

export interface ReadRow {
  id: string
  thread: string
}

export type DismissPayload = Record<string, string>

/** The silent push for the rows one write turned read; null = no push. */
export function dismissPayload(input: {
  rows: readonly ReadRow[]
  all: boolean
  teamId: string | null
}): DismissPayload | null {
  if (input.rows.length === 0) return null
  const keys = NOTIFICATION_DELIVERY_KEYS
  const base = { type: keys.dismissType }
  const everything = (): DismissPayload =>
    input.teamId ? { ...base, [keys.all]: `1`, [keys.teamId]: input.teamId } : { ...base, [keys.all]: `1` }
  if (input.all) return everything()
  const ids = [...new Set(input.rows.map((row) => row.id))]
  if (ids.length <= NOTIFICATION_DELIVERY_LIMITS.maxIds) return { ...base, [keys.ids]: ids.join(`,`) }
  const threads = [...new Set(input.rows.map((row) => row.thread))]
  if (threads.length <= NOTIFICATION_DELIVERY_LIMITS.maxThreads) return { ...base, [keys.threads]: threads.join(`,`) }
  return everything()
}

export interface DeliveredNotification {
  notificationId: string | null
  thread: string | null
  teamId: string | null
}

const splitList = (value: string | undefined): string[] =>
  value ? value.split(`,`).filter((part) => part.length > 0) : []

/** Does a dismiss push remove this delivered notification? */
export function dismissMatches(payload: Record<string, string>, delivered: DeliveredNotification): boolean {
  const keys = NOTIFICATION_DELIVERY_KEYS
  if (payload.type !== keys.dismissType) return false
  if (payload[keys.all] === `1`) {
    const teamId = payload[keys.teamId]
    return !teamId || delivered.teamId === teamId
  }
  if (delivered.notificationId && splitList(payload[keys.ids]).includes(delivered.notificationId)) return true
  return delivered.thread !== null && splitList(payload[keys.threads]).includes(delivered.thread)
}

// ── Source switches ───────────────────────────────────────────────────────

/** `assistant` | `temporary` | `agent:<agentId>`. */
export type NotificationSource = string

export interface SourceSession {
  agentId: string | null
  agentRole: string | null
  startedByAgentId: string | null
  startedByAgentRole: string | null
}

/** The source of a row naming a session, read off the session's ROOT. */
export function notificationSource(root: SourceSession): NotificationSource {
  const agentId = root.agentId ?? root.startedByAgentId
  const role = root.agentId ? root.agentRole : root.startedByAgentRole
  if (!agentId) return `temporary`
  return role === `assistant` ? `assistant` : `agent:${agentId}`
}

export interface SourcePrefs {
  assistant?: boolean
  temporary?: boolean
  agents?: Record<string, boolean>
}

export function sourceAllowed(
  prefs: SourcePrefs | null | undefined,
  source: NotificationSource | null
): boolean {
  if (!source || !prefs) return true
  if (source === `assistant`) return prefs.assistant !== false
  if (source === `temporary`) return prefs.temporary !== false
  if (source.startsWith(`agent:`)) return prefs.agents?.[source.slice(`agent:`.length)] !== false
  return true
}

/** The prefs after one switch flips (`key` = a settings row's key). */
export function toggleSourcePref(prefs: SourcePrefs, key: string, on: boolean): SourcePrefs {
  if (key === `assistant` || key === `temporary`) return { ...prefs, [key]: on }
  if (key.startsWith(`agent:`)) {
    return { ...prefs, agents: { ...prefs.agents, [key.slice(`agent:`.length)]: on } }
  }
  return prefs
}

// ── Settings ──────────────────────────────────────────────────────────────

export interface SettingsTeam {
  id: string
  name: string
}

export interface SettingsAgent {
  id: string
  teamId: string
  name: string
  role: string
}

export interface SettingsRow {
  key: string
  label: string
  detail: string | null
}

export interface SettingsGroup {
  band: string
  rows: SettingsRow[]
}

const byName = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0)

export function settingsRows(teams: readonly SettingsTeam[], agents: readonly SettingsAgent[]): SettingsGroup[] {
  const copy = NOTIFICATION_DELIVERY_COPY
  const groups: SettingsGroup[] = [
    {
      band: copy.sessionsBand,
      rows: [
        { key: `assistant`, label: copy.assistant, detail: null },
        { key: `temporary`, label: copy.temporary, detail: null },
      ],
    },
  ]
  const teamNames = new Map(teams.map((team) => [team.id, team.name]))
  const listed = agents
    .filter((agent) => agent.role !== `assistant` && teamNames.has(agent.teamId))
    .map((agent) => ({ agent, team: teamNames.get(agent.teamId) ?? `` }))
    .sort((a, b) => byName(a.team, b.team) || byName(a.agent.name, b.agent.name) || byName(a.agent.id, b.agent.id))
  if (listed.length > 0) {
    const many = teams.length > 1
    groups.push({
      band: copy.agentsBand,
      rows: listed.map(({ agent, team }) => ({
        key: `agent:${agent.id}`,
        label: agent.name,
        detail: many ? team : null,
      })),
    })
  }
  return groups
}

// ── Android channels ──────────────────────────────────────────────────────

export interface AndroidChannel {
  id: string
  name: string | null
  group: string | null
}

export function androidChannel(source: string | null, sourceName: string | null): AndroidChannel {
  const channels = NOTIFICATION_CHANNELS
  if (source === `assistant`) return { ...channels.assistant, group: null }
  if (source === `temporary`) return { ...channels.temporary, group: null }
  if (source?.startsWith(`agent:`)) {
    return {
      id: `${channels.agentPrefix}${source.slice(`agent:`.length)}`,
      name: sourceName || channels.agentsGroup.name,
      group: channels.agentsGroup.id,
    }
  }
  return { id: channels.fallback, name: null, group: null }
}
