// VAPP-33: steer over eNet's chat channel (fixtures/steer-enet.json is the
// spec). One chat message = one envelope `{t:"exp-steer", v:1, sessionId,
// frames}` carrying the steer wire frames unchanged; the device serves the
// join replay from its journal and fans live activity out per viewer.
// Mirrors: crates/steer `enet_steer.rs` (device + desktop viewer), the web
// steer store, iOS and Android.

import fixture from "../fixtures/steer-enet.json" with { type: "json" }

export const STEER_ENET_TAG = fixture.tag
export const STEER_ENET_VERSION = fixture.version
export const STEER_ENET_BATCH_MAX_BYTES = fixture.batchMaxBytes
export const STEER_ENET_CHAT_MAX_BYTES = fixture.chatMaxBytes
export const STEER_ENET_KEEPALIVE_MS = fixture.keepaliveMs
export const STEER_ENET_STALE_MS = fixture.staleMs
export const STEER_ENET_JOIN_TIMEOUT_MS = fixture.joinTimeoutMs
export const STEER_ENET_SEND_TIMEOUT_MS = fixture.sendTimeoutMs

export type SteerFrame = { t: string } & Record<string, unknown>

export type SteerEnvelope = {
  t: string
  v: number
  sessionId: string
  frames: SteerFrame[]
}

/** One envelope's chat body. */
export function encodeSteerEnvelope(sessionId: string, frames: SteerFrame[]): Uint8Array {
  return new TextEncoder().encode(
    JSON.stringify({ t: STEER_ENET_TAG, v: STEER_ENET_VERSION, sessionId, frames })
  )
}

/** A chat body (bytes or text) → the envelope, or null when it is none. */
export function parseSteerEnvelope(body: Uint8Array | string): SteerEnvelope | null {
  const text = typeof body === `string` ? body : new TextDecoder().decode(body)
  if (!text.startsWith(`{`)) return null
  let value: unknown
  try {
    value = JSON.parse(text)
  } catch {
    return null
  }
  return steerEnvelopeFrom(value)
}

/** The parse rules over an already-parsed JSON value. */
export function steerEnvelopeFrom(value: unknown): SteerEnvelope | null {
  if (!value || typeof value !== `object` || Array.isArray(value)) return null
  const v = value as Record<string, unknown>
  if (v.t !== STEER_ENET_TAG || v.v !== STEER_ENET_VERSION) return null
  if (typeof v.sessionId !== `string` || v.sessionId.length === 0) return null
  if (!Array.isArray(v.frames) || v.frames.length === 0) return null
  for (const frame of v.frames) {
    if (!frame || typeof frame !== `object` || Array.isArray(frame)) return null
    if (typeof (frame as Record<string, unknown>).t !== `string`) return null
  }
  return { t: v.t, v: v.v, sessionId: v.sessionId, frames: v.frames as SteerFrame[] }
}

/** The device's access rule for one envelope's sender. */
export function steerViewerAllowed(input: {
  viewerUserId: string | null
  starterUserId: string
  shared: boolean
}): boolean {
  if (!input.viewerUserId) return false
  return input.viewerUserId === input.starterUserId || input.shared
}

/**
 * The device's batching over serialized frame sizes: groups of frame
 * indexes, one envelope each, plus the frames too big for any envelope.
 * `overhead` = the serialized envelope with an empty frames array.
 */
export function steerBatches(
  sizes: number[],
  limits: { overhead: number; batchMaxBytes?: number; chatMaxBytes?: number }
): { groups: number[][]; dropped: number[] } {
  const batchMax = limits.batchMaxBytes ?? STEER_ENET_BATCH_MAX_BYTES
  const chatMax = limits.chatMaxBytes ?? STEER_ENET_CHAT_MAX_BYTES
  const groups: number[][] = []
  const dropped: number[] = []
  let current: number[] = []
  let bytes = limits.overhead
  sizes.forEach((size, index) => {
    if (limits.overhead + size > chatMax) {
      dropped.push(index)
      return
    }
    const next = bytes + size + (current.length > 0 ? 1 : 0)
    if (current.length > 0 && next > batchMax) {
      groups.push(current)
      current = [index]
      bytes = limits.overhead + size
      return
    }
    current.push(index)
    bytes = next
  })
  if (current.length > 0) groups.push(current)
  return { groups, dropped }
}

/** VAPP-169: what a viewer is showing when a redial trigger fires. */
export type SteerRedialPhase = `offline` | `ended` | `starting`
export type SteerRedialTrigger = `presenceOnline` | `presenceOffline` | `staleTimer` | `wakeup`

/**
 * VAPP-169: whether a trigger redials (discovery + `join`). The
 * device-offline `ended` is not terminal: presence online, a wakeup and the
 * stale timer all retry it; a `bye`-ended transcript never redials; a
 * `starting` row cuts its backoff short on presence online or a wakeup.
 */
export function steerOfflineRedial(phase: SteerRedialPhase, trigger: SteerRedialTrigger): boolean {
  if (trigger === `presenceOffline` || phase === `ended`) return false
  if (phase === `offline`) return true
  return trigger !== `staleTimer`
}

export const STEER_ENET_LOST_GRACE_MS = fixture.lostBanner.graceMs
export const STEER_ENET_LOST_FAILED_REDIALS = fixture.lostBanner.failedRedials

/**
 * EXP-1487: whether a dropped live feed shows "Connection lost". The drop
 * redials silently; the banner waits for `graceMs` down AND
 * `failedRedials` failed redials since the drop.
 */
export function steerShowConnectionLost(downMs: number, failedRedials: number): boolean {
  return downMs >= STEER_ENET_LOST_GRACE_MS && failedRedials >= STEER_ENET_LOST_FAILED_REDIALS
}

export const STEER_ENET_NO_FEED_IDLE = fixture.noFeedIdle

/** #EXP-1703: the empty state of a live row whose device is offline. */
export function steerNoFeedAway(deviceName: string | null | undefined): { title: string; text: string } {
  return {
    title: fixture.noFeedAway.title.replace(`{device}`, deviceName || fixture.noFeedAway.device),
    text: fixture.noFeedAway.text,
  }
}

/** EXP-1487: what an empty transcript says while NO FEED holds. */
export type SteerNoFeedState = `offline` | `away` | `starting` | `idle` | `waiting`

/**
 * EXP-1487: an ended row = the device-offline copy; #EXP-1703: a live row
 * whose device is offline = `away`; a busy row = starting; #EXP-2000 a row
 * no device picked up yet (`started` false: `startedAt` null) = `waiting`
 * (the placeholder says nothing); anything else (idle, waiting on a
 * question) = the idle empty state, never "starting".
 */
export function steerNoFeedState(row: { ended: boolean; busy: boolean; deviceOffline?: boolean; started?: boolean }): SteerNoFeedState {
  if (row.ended) return `offline`
  if (row.deviceOffline) return `away`
  if (row.busy) return `starting`
  return row.started === false ? `waiting` : `idle`
}

/** Android 0.14.134: how many consumed inputs a no-feed placeholder shows. */
export const STEER_NO_FEED_INPUTS_MAX = fixture.noFeedInputsMax

/** One synced message of the session, as `steerNoFeedInputs` reads it. */
export type SteerNoFeedInputRow = {
  id: string
  authorKind: string
  card: unknown
  consumedAt: string | null
  createdAt: string
}

/**
 * Android 0.14.134 (steer-enet.json SENT INPUTS): under a no-feed
 * placeholder of a live row, the session's card-less person inputs the
 * device already consumed (a not-consumed one is the Dock's queue), oldest
 * first (createdAt, then id), the newest [max] of them.
 */
export function steerNoFeedInputs<T extends SteerNoFeedInputRow>(
  messages: readonly T[],
  max: number = STEER_NO_FEED_INPUTS_MAX
): T[] {
  const inputs = messages
    .filter((m) => m.authorKind === `user` && (m.card === null || m.card === undefined) && m.consumedAt !== null)
    .sort((a, b) => (a.createdAt < b.createdAt ? -1 : a.createdAt > b.createdAt ? 1 : a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
  return inputs.slice(Math.max(0, inputs.length - max))
}

/** #FEED-260: how many synced messages a not-live transcript appends. */
export const STEER_SYNCED_TAIL_MAX = fixture.syncedTailMax

/** One row of the viewer's feed as `steerSyncedTail` reads it: a main-lane
 *  narration (`agent`), an input (`user`) or anything else (null), its text
 *  and its known time (ISO; a narration's first fragment, an input, a
 *  settled tool; null = unknown). */
export type SteerTailFeedRow = { author: `agent` | `user` | null; text: string | null; at: string | null }

/** One synced message of the session, as `steerSyncedTail` reads it. */
export type SteerSyncedMessage = SteerNoFeedInputRow & { body: string }

function epochMs(iso: string | null): number | null {
  if (iso === null) return null
  const ms = Date.parse(iso)
  return Number.isNaN(ms) ? null : ms
}

/**
 * #FEED-260 (steer-enet.json SYNCED TAIL): the synced messages a NOT-live
 * transcript draws after its feed, oldest first (createdAt, then id), the
 * newest [max]: card-less with a non-blank body, the agent's or a person's
 * CONSUMED input, later than the feed's newest known row time, and not
 * already held by the feed (an agent line equal to, a prefix of or prefixed
 * by a narration; an input equal to a feed input). An empty feed takes the
 * tail only while its live transcript cannot be loaded (`unreachable`, the
 * OFFLINE LINE's condition) and only when it holds an agent line (else
 * SENT INPUTS keeps the placeholder). A live feed = none.
 */
export function steerSyncedTail<T extends SteerSyncedMessage>(input: {
  live: boolean
  feed: readonly SteerTailFeedRow[]
  messages: readonly T[]
  max?: number
  /** The live transcript cannot be loaded (consulted for an empty feed). */
  unreachable?: boolean
}): T[] {
  if (input.live) return []
  if (input.feed.length === 0 && input.unreachable !== true) return []
  const max = input.max ?? STEER_SYNCED_TAIL_MAX
  let feedAt: number | null = null
  for (const row of input.feed) {
    const at = epochMs(row.at)
    if (at !== null && (feedAt === null || at > feedAt)) feedAt = at
  }
  if (input.feed.length > 0 && feedAt === null) return []
  const narrations = input.feed.filter((row) => row.author === `agent` && row.text).map((row) => (row.text ?? ``).trim())
  const inputs = new Set(input.feed.filter((row) => row.author === `user` && row.text).map((row) => (row.text ?? ``).trim()))
  const tail = input.messages
    .filter((m) => {
      if (m.card !== null && m.card !== undefined) return false
      const body = m.body.trim()
      if (body === ``) return false
      if (m.authorKind === `agent`) {
        if (narrations.some((text) => text !== `` && (text.startsWith(body) || body.startsWith(text)))) return false
      } else if (m.authorKind === `user`) {
        if (m.consumedAt === null || inputs.has(body)) return false
      } else {
        return false
      }
      if (feedAt === null) return true
      const created = epochMs(m.createdAt)
      return created !== null && created > feedAt
    })
    .sort((a, b) => (a.createdAt < b.createdAt ? -1 : a.createdAt > b.createdAt ? 1 : a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
  if (input.feed.length === 0 && !tail.some((m) => m.authorKind === `agent`)) return []
  return tail.slice(Math.max(0, tail.length - max))
}

/**
 * #FEED-260 (steer-enet.json OFFLINE LINE): whether a not-live transcript
 * showing rows ends with THE inline marker `noFeedAway.title`: an ended row
 * whose last dial said NO FEED, a row not ended whose device is offline.
 */
export function steerOfflineLine(input: {
  live: boolean
  rows: number
  ended: boolean
  noFeed: boolean
  deviceOffline: boolean
}): boolean {
  if (input.live || input.rows === 0) return false
  return input.ended ? input.noFeed : input.deviceOffline
}

/** The synced row fields `steerRowWakes` reads. */
export type SteerWakeRow = { status: string; busy: boolean; deviceId: string | null }

/**
 * Android 0.14.134 (steer-enet.json ROW WAKE): whether a synced row change
 * cuts a `starting` row's backoff short: the device took it (status leaves
 * `queued` or turns `running`), a turn started, or it moved; never an
 * ending, never the first row seen.
 */
export function steerRowWakes(from: SteerWakeRow | null, to: SteerWakeRow): boolean {
  if (from === null || to.status === `ended`) return false
  if (from.status === `queued` && to.status !== `queued`) return true
  if (from.status !== `running` && to.status === `running`) return true
  if (!from.busy && to.busy) return true
  return (from.deviceId ?? ``) !== (to.deviceId ?? ``)
}

// ── P2P stabilisation: the sealed relay FALLBACK (fixture `fallback`) ───────

export const STEER_FALLBACK_STALL_MS = fixture.fallback.stallMs
export const STEER_TUNNEL_MAX_DATA_BYTES = fixture.fallback.tunnelMaxDataBytes
export const STEER_RELAY_RETRY_MS = fixture.fallback.relayRetryMs
export const STEER_TUNNEL_ERROR_CODES = fixture.fallback.tunnelErrorCodes
export const STEER_STARTING_RETRY = fixture.fallback.startingRetry

/** #EXP-1678 (`startingRetryCases`): the cap of the n-th consecutive NO FEED
 *  redial; the wait is a random delay in [cap/2, cap) (equal jitter). */
export function steerStartingRetryCapMs(retries: number): number {
  return Math.min(STEER_STARTING_RETRY.baseMs * 2 ** retries, STEER_STARTING_RETRY.maxMs)
}

/** #EXP-1678: the wait itself (`random` in [0, 1)). */
export function steerStartingRetryDelayMs(retries: number, random: number = Math.random()): number {
  const cap = steerStartingRetryCapMs(retries)
  return cap / 2 + random * (cap / 2)
}

export type SteerCarrier = `enet` | `relay`
export type SteerCarrierState = { active: `none` | SteerCarrier; relay: `closed` | `open` }
export type SteerCarrierEvent =
  | `start`
  | `stall`
  | `enetNoFeed`
  | `enetRefused`
  | `enetReset`
  | `enetSynced`
  | `enetDropped`
  | `relayReset`
  | `relaySynced`
  | `relayDropped`
  | `relayUnavailable`
export type SteerCarrierAction =
  | `dialEnet`
  | `armStall`
  | `openRelay`
  | `rejoinRelay`
  | `closeRelay`
  | `reopenRelayLater`
  | `redialEnetLater`
  | `apply`
  | `drop`

/**
 * The one carrier rule every viewer runs: eNet first, the sealed relay only
 * when eNet stalls (`stallMs`), has no feed or drops; eNet keeps redialing
 * while the relay is active and takes over on its `activity_reset`. Frames
 * apply only from the active carrier (`apply`), anything else is `drop`ped.
 * `enetReset`/`relayReset` = that carrier's `activity_reset`; the `*Synced`
 * events stand for every later frame of that carrier (synced, live, bye).
 */
export function steerCarrierStep(
  state: SteerCarrierState,
  event: SteerCarrierEvent
): { state: SteerCarrierState; actions: SteerCarrierAction[] } {
  const { active, relay } = state
  const at = (a: SteerCarrierState[`active`], r: SteerCarrierState[`relay`], actions: SteerCarrierAction[]) => ({
    state: { active: a, relay: r },
    actions,
  })
  switch (event) {
    case `start`:
      return at(`none`, `closed`, [`dialEnet`, `armStall`])
    case `stall`:
      return active === `none` && relay === `closed` ? at(`none`, `open`, [`openRelay`]) : at(active, relay, [])
    case `enetNoFeed`:
      if (active === `none` && relay === `closed`) return at(`none`, `open`, [`openRelay`, `redialEnetLater`])
      return at(active, relay, [`redialEnetLater`])
    case `enetRefused`:
      // #EXP-1678: the device itself said no; the relay reaches the same device.
      return at(active, relay, active === `relay` ? [`redialEnetLater`] : [])
    case `enetReset`:
      return at(`enet`, relay, [`apply`])
    case `enetSynced`:
      if (active !== `enet`) return at(active, relay, [`drop`])
      return relay === `open` ? at(`enet`, `closed`, [`apply`, `closeRelay`]) : at(`enet`, relay, [`apply`])
    case `enetDropped`:
      if (active !== `enet`) return at(active, relay, [`redialEnetLater`])
      return relay === `open`
        ? at(`none`, `open`, [`rejoinRelay`, `redialEnetLater`])
        : at(`none`, `open`, [`openRelay`, `redialEnetLater`])
    case `relayReset`:
      return active === `enet` ? at(active, relay, [`drop`]) : at(`relay`, relay, [`apply`])
    case `relaySynced`:
      return active === `relay` ? at(active, relay, [`apply`]) : at(active, relay, [`drop`])
    case `relayDropped`:
      return active === `enet` ? at(`enet`, `closed`, []) : at(`none`, `closed`, [`reopenRelayLater`])
    case `relayUnavailable`:
      return at(active, `closed`, [])
  }
}

// ── Connection analytics (fixture `connStats`) ───────────────────────────────

const stats = fixture.connStats
export const STEER_CONN_STATS = stats

export type SteerConnPlatform = (typeof stats.platforms)[number]
export type SteerConnPath = `direct` | `iroh_relay` | `server_fallback`
export type SteerConnData = {
  phase: `settled` | `ended`
  platform: string
  version: string
  outcome: `ok` | `failed`
  path?: SteerConnPath
  firstFrameMs?: number
  errorClass?: string
  fallbacks?: number
  reconnects?: number
  durationMs?: number
  failures?: Record<string, number>
}
export type SteerConnEvent = {
  v: 1
  id: string
  kind: `steer.view` | `steer.serve`
  at: number
  source: `steer-viewer` | `steer-device`
  data: SteerConnData
}

const DATA_KEYS = new Set([
  `phase`,
  `platform`,
  `version`,
  `outcome`,
  `path`,
  `firstFrameMs`,
  `errorClass`,
  `fallbacks`,
  `reconnects`,
  `durationMs`,
  `failures`,
])
const EVENT_KEYS = new Set([`v`, `id`, `kind`, `at`, `source`, `data`])
const isCount = (v: unknown) => typeof v === `number` && Number.isInteger(v) && v >= 0 && v <= 2_147_483_647

/** The server's validation of one posted event: anything off-schema (an
 *  extra key included: content must never ride along) is refused whole. */
export function steerConnEventValid(value: unknown): value is SteerConnEvent {
  if (!value || typeof value !== `object` || Array.isArray(value)) return false
  const e = value as Record<string, unknown>
  if (Object.keys(e).some((k) => !EVENT_KEYS.has(k))) return false
  if (e.v !== 1 || typeof e.id !== `string` || e.id.length === 0 || e.id.length > 64) return false
  if (!stats.kinds.includes(e.kind as string) || !stats.sources.includes(e.source as string)) return false
  if (typeof e.at !== `number` || !Number.isInteger(e.at) || e.at <= 0) return false
  const d = e.data as Record<string, unknown> | null
  if (!d || typeof d !== `object` || Array.isArray(d)) return false
  if (Object.keys(d).some((k) => !DATA_KEYS.has(k))) return false
  if (!stats.phases.includes(d.phase as string) || !stats.platforms.includes(d.platform as string)) return false
  if (typeof d.version !== `string` || d.version.length === 0 || d.version.length > stats.versionMax) return false
  if (!stats.outcomes.includes(d.outcome as string)) return false
  if (d.path !== undefined && !stats.paths.includes(d.path as string)) return false
  if (d.outcome === `ok` && d.path === undefined) return false
  if (d.errorClass !== undefined && !stats.errorClasses.includes(d.errorClass as string)) return false
  for (const k of [`firstFrameMs`, `fallbacks`, `reconnects`, `durationMs`] as const) {
    if (d[k] !== undefined && !isCount(d[k])) return false
  }
  if (d.failures !== undefined) {
    const f = d.failures as Record<string, unknown>
    if (!f || typeof f !== `object` || Array.isArray(f)) return false
    for (const [k, n] of Object.entries(f)) {
      if (!stats.errorClasses.includes(k) || !isCount(n)) return false
    }
  }
  return true
}

/**
 * #EXP-1678: the errorClass a `steer.view` event carries (`errorClassCases`).
 * A FAILED view always names one: `background` while the app or page is in
 * the background (its carriers count no failure then), else the last carrier
 * failure, else `dialing` (nothing answered or failed yet). An OK view keeps
 * its last failure, if any.
 */
export function steerViewErrorClass(view: {
  ok: boolean
  lastError: string | null | undefined
  background: boolean
}): string | undefined {
  if (view.ok) return view.lastError ?? undefined
  if (view.background) return `background`
  return view.lastError ?? `dialing`
}

export type SteerConnRow = Pick<SteerConnData, `phase` | `outcome` | `path` | `firstFrameMs` | `fallbacks`> & {
  errorClass?: string
  durationMs?: number
}
export type SteerConnAggregate = {
  views: number
  successRate: number | null
  p50FirstFrameMs: number | null
  p95FirstFrameMs: number | null
  fallbackRate: number | null
}

/** Nearest-rank percentile (rank = ceil(p * n), 1-based) of sorted values. */
function nearestRank(sorted: number[], p: number): number | null {
  if (sorted.length === 0) return null
  return sorted[Math.max(0, Math.ceil(p * sorted.length) - 1)] ?? null
}

/** The dashboard's numbers over one platform/version bucket (`$aggregateRule`). */
export function steerConnAggregate(rows: SteerConnRow[]): SteerConnAggregate {
  const settled = rows.filter((r) => r.phase === `settled`)
  const views = settled.length
  if (views === 0) return { views, successRate: null, p50FirstFrameMs: null, p95FirstFrameMs: null, fallbackRate: null }
  const ok = settled.filter((r) => r.outcome === `ok`)
  const firsts = ok.map((r) => r.firstFrameMs).filter((v): v is number => typeof v === `number`).sort((a, b) => a - b)
  const fallbackSettles = settled.filter((r) => r.path === `server_fallback`).length
  const fallbackEnded = rows.filter(
    (r) => r.phase === `ended` && (r.fallbacks ?? 0) > 0 && r.path !== `server_fallback`
  ).length
  return {
    views,
    successRate: ok.length / views,
    p50FirstFrameMs: nearestRank(firsts, 0.5),
    p95FirstFrameMs: nearestRank(firsts, 0.95),
    fallbackRate: (fallbackSettles + fallbackEnded) / views,
  }
}

// ---------------------------------------------------------------------------
// #EDE-96 SEARCH (steer-enet.json `search`): the device answers a transcript
// search over the session's journal. The pure rule, replayed by every mirror.
// ---------------------------------------------------------------------------

export const STEER_SEARCH_QUERY_MAX_CHARS = fixture.search.queryMaxChars
export const STEER_SEARCH_HITS_MAX = fixture.search.hitsMax
export const STEER_SEARCH_SCAN_MAX_BYTES = fixture.search.scanMaxBytes
export const STEER_SEARCH_SNIPPET_CHARS = fixture.search.snippetChars
export const STEER_SEARCH_SNIPPET_LEAD_CHARS = fixture.search.snippetLeadChars

/** What a hit points at: the row kind a viewer draws it in. */
export type SteerSearchKind = `user_message` | `narration` | `tool` | `tool_output`

/** One match: `seq` = the ROW's seq (a tool's output = its `tool` row),
 *  `matchStart`/`matchLen` in code points of `snippet`. */
export type SteerSearchHit = { seq: number; kind: SteerSearchKind; snippet: string; matchStart: number; matchLen: number }

/** The viewer's ask (`beforeSeq` absent = from the newest event). */
export type SteerSearchFrame = { t: `search`; requestId: string; query: string; beforeSeq?: number; limit: number }

/** The device's answer, to the asker only. */
export type SteerSearchResultFrame = {
  t: `search_result`
  requestId: string
  hits: SteerSearchHit[]
  total?: number
  done: boolean
  scannedToSeq: number
}

/** The answer's body (`steerSearch`). */
export type SteerSearchResult = Omit<SteerSearchResultFrame, `t` | `requestId`>

/** One journal event as `steerSearch` reads it (the wire `activity` event). */
export type SteerSearchEvent = { kind: string } & Record<string, unknown>

const SEARCH_SPACE = new Set([` `, `\t`, `\n`, `\r`, `\f`, `\v`])

/** Every run of ASCII whitespace (space, tab, LF, CR, FF, VT) = one space, trimmed. */
export function steerSearchCollapse(text: string): string {
  let out = ``
  let space = false
  for (const ch of text) {
    if (SEARCH_SPACE.has(ch)) {
      space = out !== ``
      continue
    }
    if (space) out += ` `
    space = false
    out += ch
  }
  return out
}

/** The fold, per code point: NFD, marks (General_Category M) dropped, lowercased.
 *  `source[i]` = the code-point index in `text` folded char `i` came from. */
export function steerSearchFold(text: string): { chars: string[]; source: number[] } {
  const chars: string[] = []
  const source: number[] = []
  Array.from(text).forEach((ch, index) => {
    for (const part of ch.normalize(`NFD`)) {
      if (/\p{M}/u.test(part)) continue
      for (const lower of part.toLowerCase()) {
        chars.push(lower)
        source.push(index)
      }
    }
  })
  return { chars, source }
}

/** The searchable text of one event, its hit kind and the row it lands on. */
function searchable(
  event: SteerSearchEvent,
  seq: number,
  toolRows: Map<string, number>
): { kind: SteerSearchKind; text: string; seq: number } | null {
  const str = (value: unknown) => (typeof value === `string` ? value : null)
  switch (event.kind) {
    case `user_message`:
    case `narration`: {
      const text = str(event.text)
      return text === null ? null : { kind: event.kind, text, seq }
    }
    case `tool`: {
      const name = str(event.name) ?? ``
      const detail = str(event.detail)
      return { kind: `tool`, text: detail ? `${name} ${detail}` : name, seq }
    }
    case `tool_update`: {
      const text = str(event.output) ?? str(event.diff)
      const row = toolRows.get(str(event.id) ?? ``)
      return text === null || row === undefined ? null : { kind: `tool_output`, text, seq: row }
    }
    default:
      return null
  }
}

/**
 * #EDE-96 (steer-enet.json SEARCH): search a journal (`events` with their
 * seqs, oldest first, as the device reads it) for `query`, newest first,
 * below `beforeSeq`, at most `limit` hits and `scanMaxBytes` of text.
 */
export function steerSearch(input: {
  events: readonly { seq: number; event: SteerSearchEvent }[]
  query: string
  beforeSeq?: number
  limit: number
  scanMaxBytes?: number
}): SteerSearchResult {
  const query = steerSearchFold(Array.from(steerSearchCollapse(input.query)).slice(0, STEER_SEARCH_QUERY_MAX_CHARS).join(``)).chars
  if (query.length === 0) return { hits: [], done: true, scannedToSeq: 0 }
  const limit = Math.min(Math.max(input.limit, 1), STEER_SEARCH_HITS_MAX)
  const scanMax = input.scanMaxBytes ?? STEER_SEARCH_SCAN_MAX_BYTES
  const toolRows = new Map<string, number>()
  for (const { seq, event } of input.events) {
    if (event.kind === `tool` && typeof event.id === `string`) toolRows.set(event.id, Math.min(seq, toolRows.get(event.id) ?? seq))
  }
  // Newest first BY SEQ (a journal read lists its latest-wins slots after the log).
  const candidates = input.events
    .filter(({ seq }) => input.beforeSeq === undefined || seq < input.beforeSeq)
    .sort((a, b) => b.seq - a.seq)
  const hits: SteerSearchHit[] = []
  const encoder = new TextEncoder()
  let scanned = 0
  let scannedToSeq = 0
  for (let index = 0; index < candidates.length; index++) {
    const { seq, event } = candidates[index]
    scannedToSeq = seq
    const found = searchable(event, seq, toolRows)
    if (found !== null) {
      const text = steerSearchCollapse(found.text)
      scanned += encoder.encode(text).length
      const hit = searchMatch(text, query)
      if (hit !== null) hits.push({ seq: found.seq, kind: found.kind, ...hit })
    }
    if ((hits.length >= limit || scanned >= scanMax) && index < candidates.length - 1) {
      return { hits, done: false, scannedToSeq }
    }
  }
  return input.beforeSeq === undefined ? { hits, total: hits.length, done: true, scannedToSeq } : { hits, done: true, scannedToSeq }
}

/** The first match of the folded `query` in `text` (collapsed), as a snippet. */
function searchMatch(text: string, query: string[]): Omit<SteerSearchHit, `seq` | `kind`> | null {
  const folded = steerSearchFold(text)
  const chars = Array.from(text)
  for (let i = 0; i + query.length <= folded.chars.length; i++) {
    let j = 0
    while (j < query.length && folded.chars[i + j] === query[j]) j++
    if (j < query.length) continue
    const start = folded.source[i]
    const end = folded.source[i + query.length - 1] + 1
    const from = Math.max(0, start - STEER_SEARCH_SNIPPET_LEAD_CHARS)
    const to = Math.min(chars.length, Math.max(from + STEER_SEARCH_SNIPPET_CHARS, end))
    const lead = from > 0 ? `…` : ``
    return {
      snippet: `${lead}${chars.slice(from, to).join(``)}${to < chars.length ? `…` : ``}`,
      matchStart: start - from + lead.length,
      matchLen: end - start,
    }
  }
  return null
}
