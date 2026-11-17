// #EDE-115 (chat-store.json `catchUp`, S4): the web's BACKGROUND CATCH-UP
// over eNet, only in the tab holding the eNet node's Web Lock (like the key
// holder's loop). Transcripts travel device to device: never the server,
// never the sealed relay tunnel.
//
// * ASKER: for every chat the account reads (each synced `sessions` row with
//   a `deviceId`), walk the events this store lacks from the device running
//   it, then the account's other installs, `concurrency` sessions at once,
//   through the byte bucket, idle-only on battery, resuming from the store's
//   `completeTo` watermarks; `minAt` = the web's RETENTION cutoff.
// * HOLDER: a device-naming `history_page` from another install is answered
//   off this store (`fullTo` = its own `completeTo`) under the join's access
//   rule, with one `history_chunk` to the asker only.
//
// The rules (have, the holder's chunk, progress, holders, the gate, the
// retry, the bucket) are the crate's, through the store's worker.

import {
  encodeSteerEnvelope,
  parseSteerEnvelope,
  STEER_ENET_CHAT_MAX_BYTES,
  type SteerFrame,
} from "@exp/domain-contract/steer-enet"

/** The catch-up's own bookkeeping file (the checked stamps). */
const INDEX = `_index`
/** The account's sessions changed: a pass after this long. */
const KICK_DEBOUNCE_MS = 3_000
/** No node, or the power gate said no: looked at again this often. */
const GATE_POLL_MS = 60_000
/** The largest chunk this holder sends (one envelope, with room for it). */
const CHUNK_MAX_BYTES = STEER_ENET_CHAT_MAX_BYTES - 4096

export type CatchUpRow = {
  id: string
  rootId: string | null
  teamId: string
  userId: string | null
  shared: boolean
  deviceId: string | null
  updatedAt: Date | string | null
  busy: boolean
}

export type CatchUpInstall = { endpointId: string; userId: string; deviceId: string | null; lastSeenAt: Date | string | null }

type Constants = { pageMax: number; concurrency: number; askTimeoutMs: number; passIntervalMs: number; endpointBackoffMs: number }
type Take = { added: number; completeTo: number | null; next: number | null; end: boolean }

/** What the loop needs (the tab's node, the store's worker, the synced rows). */
export type CatchUpDeps = {
  userId: string
  node: {
    readonly running: boolean
    readonly endpointId: string | undefined
    sendChat: (to: string, body: Uint8Array) => Promise<void>
    onChat: (listener: (from: string, body: Uint8Array) => void) => () => void
  }
  /** One op on the store's worker (undefined = no store here). */
  store: <T>(op: string, ...args: unknown[]) => Promise<T | undefined>
  sessions: () => CatchUpRow[]
  onSessionsChange: (listener: () => void) => () => void
  installs: (teamId: string) => Promise<CatchUpInstall[]>
  /** `ac` | `battery` | `unknown`. */
  power: () => string
  idleMs: () => number
  now?: () => number
}

function iso(value: Date | string | null): string {
  if (value === null) return ``
  return typeof value === `string` ? value : value.toISOString()
}

function stamp(row: CatchUpRow): string {
  return `${row.deviceId}|${iso(row.updatedAt)}`
}

const checkedKey = (sessionId: string) => `catchup.checked.${sessionId}`
const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms))

/** Starts the loop; the returned function stops it. */
export function startCatchUp(deps: CatchUpDeps): () => void {
  const now = deps.now ?? Date.now
  let stopped = false
  let wake: (() => void) | undefined
  let kickTimer: ReturnType<typeof setTimeout> | undefined
  let requests = 0
  const pending = new Map<string, (answer: { frame: SteerFrame; bytes: number }) => void>()
  const backoff = new Map<string, number>()
  const retries = new Map<string, { attempts: number; next: number }>()

  const kick = () => {
    clearTimeout(kickTimer)
    kickTimer = setTimeout(() => wake?.(), KICK_DEBOUNCE_MS)
  }
  const nap = (ms: number) =>
    new Promise<void>((resolve) => {
      const timer = setTimeout(done, ms)
      function done() {
        clearTimeout(timer)
        if (wake === done) wake = undefined
        resolve()
      }
      wake = done
    })

  // ── The wire ─────────────────────────────────────────────────────────────

  const offChat = deps.node.onChat((from, body) => {
    const envelope = parseSteerEnvelope(body)
    if (!envelope) return
    for (const frame of envelope.frames) {
      if (frame.t === `history_chunk` && typeof frame.requestId === `string`) {
        const answer = pending.get(frame.requestId)
        if (answer) {
          pending.delete(frame.requestId)
          answer({ frame, bytes: body.length })
        }
      } else if (frame.t === `history_page` && typeof frame.deviceId === `string`) {
        void serve(from, envelope.sessionId, frame).catch(() => {})
      }
    }
  })
  const offSessions = deps.onSessionsChange(kick)

  /** THE HOLDER, under the join's access rule. */
  const serve = async (from: string, sessionId: string, page: SteerFrame) => {
    const row = deps.sessions().find((r) => r.id === sessionId)
    let chunk: string | null | undefined
    if (!row) chunk = await deps.store<string | null>(`refusal`, page, `no_such_session`)
    else {
      const asker = (await deps.installs(row.teamId).catch(() => [])).find((i) => i.endpointId === from)?.userId
      const allowed = asker !== undefined && (asker === row.userId || row.shared)
      chunk = allowed
        ? await deps.store<string | null>(`answer`, row.rootId ?? row.id, sessionId, page, CHUNK_MAX_BYTES)
        : await deps.store<string | null>(`refusal`, page, `unauthorized`)
    }
    if (!chunk || stopped) return
    await deps.node.sendChat(from, encodeSteerEnvelope(sessionId, [JSON.parse(chunk) as SteerFrame]))
  }

  /** THE ASKER: one page from `peer`. */
  const askPage = async (peer: string, sessionId: string, frame: SteerFrame, timeoutMs: number) => {
    const requestId = `cu-${now().toString(36)}-${++requests}`
    const answered = new Promise<{ frame: SteerFrame; bytes: number } | null>((resolve) => {
      pending.set(requestId, resolve)
      setTimeout(() => resolve(null), timeoutMs)
    })
    try {
      await deps.node.sendChat(peer, encodeSteerEnvelope(sessionId, [{ ...frame, requestId }]))
      const answer = await answered
      if (!answer) throw new Error(`no answer in time`)
      if (typeof answer.frame.error === `string`) throw new Error(answer.frame.error)
      return answer
    } finally {
      pending.delete(requestId)
    }
  }

  // ── The walk ─────────────────────────────────────────────────────────────

  /** WALK one session over its holders; true = caught up from its runner. */
  const walk = async (row: CatchUpRow, installs: CatchUpInstall[], c: Constants): Promise<boolean> => {
    const self = deps.node.endpointId
    if (!self || !row.deviceId) return false
    const rootId = row.rootId ?? row.id
    const deviceId = row.deviceId
    const at = now()
    for (const [endpoint, until] of backoff) if (until <= at) backoff.delete(endpoint)
    const holders =
      (await deps.store<{ endpointId: string; runner: boolean }[]>(
        `holders`,
        installs.map((i) => ({ endpointId: i.endpointId, userId: i.userId, deviceId: i.deviceId, lastSeenAt: i.lastSeenAt === null ? null : iso(i.lastSeenAt) })),
        deviceId,
        self,
        deps.userId,
        [...backoff.keys()]
      )) ?? []
    let complete: number | null = null
    for (const holder of holders) {
      let cursor: number | null | undefined = undefined
      let last: number | null = null
      let finished = false
      for (;;) {
        if (stopped) return false
        const wait = (await deps.store<number>(`bucketWait`)) ?? 0
        if (wait > 0) await sleep(wait)
        type Ask = { afterSeq: number | null; have: [number, number][]; minAt: number }
        const ask: Ask | undefined = await deps.store<Ask>(`ask`, rootId, row.id, deviceId, cursor)
        if (!ask) return false
        if (cursor === undefined) complete = ask.afterSeq
        cursor = ask.afterSeq
        const frame: SteerFrame = { t: `history_page`, deviceId, limit: c.pageMax, have: ask.have, minAt: ask.minAt }
        if (cursor !== null) frame.afterSeq = cursor
        let answer: { frame: SteerFrame; bytes: number }
        try {
          answer = await askPage(holder.endpointId, row.id, frame, c.askTimeoutMs)
        } catch {
          backoff.set(holder.endpointId, now() + c.endpointBackoffMs)
          break
        }
        await deps.store(`bucketSpend`, answer.bytes)
        const took: Take | undefined = await deps.store<Take>(`take`, rootId, row.id, deviceId, cursor, answer.frame)
        if (!took) return false
        last = typeof answer.frame.lastSeq === `number` ? answer.frame.lastSeq : null
        complete = took.completeTo
        if (took.end) {
          finished = true
          break
        }
        cursor = took.next
      }
      // caught_up: the runner holds nothing past what this store holds whole.
      if (finished && holder.runner && (last === null || (complete !== null && complete >= last))) return true
    }
    return false
  }

  /** One pass: every session due, newest chat first, `concurrency` at once. */
  const pass = async (c: Constants) => {
    const rows = deps
      .sessions()
      .filter((row) => !!row.deviceId)
      .sort((a, b) => iso(b.updatedAt).localeCompare(iso(a.updatedAt)) || a.id.localeCompare(b.id))
    const at = now()
    const checked = (await deps.store<(string | null)[]>(`metas`, INDEX, rows.map((row) => checkedKey(row.id)))) ?? []
    const due = rows.filter((row, ix) => {
      const retry = retries.get(row.id)
      if (retry && retry.next > at) return false
      return row.busy || checked[ix] !== stamp(row)
    })
    if (due.length === 0) return
    const installs = new Map<string, Promise<CatchUpInstall[]>>()
    const teamInstalls = (teamId: string) => {
      let list = installs.get(teamId)
      if (!list) {
        list = deps.installs(teamId).catch(() => [])
        installs.set(teamId, list)
      }
      return list
    }
    let next = 0
    const lane = async () => {
      while (!stopped && next < due.length) {
        const row = due[next++]!
        const caught = await walk(row, await teamInstalls(row.teamId), c).catch(() => false)
        if (caught) {
          retries.delete(row.id)
          if (!row.busy) await deps.store(`setMeta`, INDEX, checkedKey(row.id), stamp(row))
        } else {
          const attempts = (retries.get(row.id)?.attempts ?? -1) + 1
          const ms = (await deps.store<number>(`retryMs`, attempts)) ?? c.passIntervalMs
          retries.set(row.id, { attempts, next: now() + ms })
        }
      }
    }
    await Promise.all(Array.from({ length: Math.max(1, c.concurrency) }, lane))
  }

  void (async () => {
    const c = await deps.store<Constants>(`constants`).catch(() => undefined)
    if (!c) return
    while (!stopped) {
      const gate = deps.node.running && ((await deps.store<boolean>(`mayRun`, deps.power(), deps.idleMs())) ?? false)
      if (!gate) {
        await nap(GATE_POLL_MS)
        continue
      }
      await pass(c).catch(() => {})
      if (!stopped) await nap(c.passIntervalMs)
    }
  })()

  return () => {
    stopped = true
    clearTimeout(kickTimer)
    offChat()
    offSessions()
    for (const answer of pending.values()) answer({ frame: { t: `history_chunk`, error: `stopped` }, bytes: 0 })
    pending.clear()
    wake?.()
  }
}
