// #FEED-295: device-to-device file transfers between ONE person's own
// machines. The server only brokers: it checks that both machines are the
// caller's own devices rows, that both run a build that can transfer (cap
// `file-transfer`) and have a live eNet install (desktop app or CLI daemon),
// logs the transfer (`device_transfers`) and queues one command per side:
// `file_send` on the source (serve `srcPath` to the destination's installs
// only) and `file_receive` on the destination (pull it into `destPath`).
// The bytes go machine to machine over eNet (QUIC, end-to-end encrypted, the
// relay as the fallback path), streamed, resumable, BLAKE3-checked per file:
// never through the server or its storage.
// Owner decision (2026-11-27): no confirmation prompt on either side; a
// transfer between the same person's own machines just runs. The machines
// still apply their own rules (crates/steer `file_transfer.rs`): the peer
// must be an install of the same person, no `..`, no symlinks followed, no
// protected paths (~/.ssh, keyrings, credentials, Exponential's data), no
// existing destination.

import { and, count, desc, eq, inArray, isNull, ne, notInArray, sql } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { db as defaultDb } from "@/db/connection"
import { deviceCommands, devices, deviceTransfers, installs, mcpTasks, sessions, type DeviceTransfer } from "@/db/schema"
import { contract } from "@exp/domain-contract"
import { insertMessage } from "@/lib/sessions/messages"
import type { Db, Tx } from "@/lib/sessions/tx"
import { DEVICE_OTHER_TEAM_MESSAGE, deviceUsableInTeam } from "@/lib/session-device-move"
import { getPersonalTeamId } from "@/lib/auth/membership"
import { endTransferTasks } from "@/lib/sessions/task-watchers"
import { formatBytes } from "@exp/emcp/tasks"

export const FILE_TRANSFER_CAP = `file-transfer`
export const FILE_SEND_KIND = `file_send`
export const FILE_RECEIVE_KIND = `file_receive`
export const TRANSFER_TERMINAL = [`done`, `failed`, `cancelled`] as const
export const TRANSFER_PATH_MAX = 4096
const SKIPPED_KEEP = 200
/** Plan decision (2026-11-27): at most this many queued or running transfers per person. */
export const TRANSFERS_LIVE_MAX = 4
/** Installs of one machine a side may talk to (newest first). */
const PEERS_MAX = 4

export const TRANSFER_COPY = {
  sameDevice: `The source and the destination are the same device.`,
  notYours: `Both devices must be your own (exponential_devices list).`,
  noCap: (label: string) => `${label} runs a build without file transfers; update it first.`,
  noInstall: (label: string) => `${label} has no P2P node registered yet; start the app or the daemon on it once.`,
  noDevice: `Name fromDeviceId and toDeviceId (one of them defaults to your session's device).`,
  notFound: `Transfer not found`,
  tooMany: `You already have ${TRANSFERS_LIVE_MAX} transfers running; wait for one to end or cancel one.`,
} as const

/** A path a machine may resolve: absolute or `~/`, no `..`, no NUL. The
 *  machine applies the real rules (protected paths, symlinks). */
export function transferPathError(path: string): string | null {
  if (!path.trim()) return `The path is empty.`
  if (path.length > TRANSFER_PATH_MAX) return `The path is too long.`
  if (path.includes(`\0`)) return `The path contains a NUL byte.`
  const absolute = path.startsWith(`/`) || path === `~` || path.startsWith(`~/`) || /^[A-Za-z]:[\\/]/.test(path)
  if (!absolute) return `Use an absolute path or one starting with ~/.`
  if (path.split(/[\\/]+/).some((part) => part === `..`)) return `The path must not contain "..".`
  return null
}

export { formatBytes }

/** The line the asking session wakes on when its transfer ends. */
export function transferEndText(
  row: Pick<DeviceTransfer, `id` | `status` | `files` | `bytes` | `skipped` | `error` | `srcPath` | `destPath`>,
  labels: { from: string; to: string }
): string {
  const route = `${labels.from}:${row.srcPath} → ${labels.to}:${row.destPath}`
  if (row.status === `done`) {
    const skipped = row.skipped?.length ? `, ${row.skipped.length} skipped (symlinks or protected paths)` : ``
    return `Transfer ${row.id} done: ${route}, ${row.files ?? 0} files, ${formatBytes(row.bytes ?? 0)}, every checksum verified${skipped}.`
  }
  if (row.status === `cancelled`) return `Transfer ${row.id} cancelled: ${route}.`
  return `Transfer ${row.id} failed: ${route}: ${row.error ?? `unknown error`}`
}

export function transferView(row: DeviceTransfer) {
  return {
    transferId: row.id,
    status: row.status,
    fromDeviceId: row.fromDeviceId,
    toDeviceId: row.toDeviceId,
    srcPath: row.srcPath,
    destPath: row.destPath,
    files: row.files,
    filesDone: row.filesDone,
    bytes: row.bytes,
    bytesDone: row.bytesDone,
    skipped: row.skipped ?? [],
    error: row.error,
    sessionId: row.sessionId,
    createdAt: row.createdAt,
    startedAt: row.startedAt,
    completedAt: row.completedAt,
  }
}

type DeviceRow = {
  id: string
  deviceId: string
  label: string
  caps: string[] | null
  lastSeenAt: Date | null
  sharedTeamIds: string[] | null
}

/** Each device row's live eNet installs (desktop app or daemon), newest first. */
async function liveEndpoints(db: Db, userId: string, rowIds: string[]): Promise<Map<string, string[]>> {
  const rows = await db
    .select({ deviceId: installs.deviceId, endpointId: installs.endpointId })
    .from(installs)
    .where(
      and(
        eq(installs.userId, userId),
        inArray(installs.deviceId, rowIds),
        inArray(installs.kind, [`desktop`, `daemon`]),
        isNull(installs.revokedAt)
      )
    )
    .orderBy(desc(installs.lastSeenAt))
  const out = new Map<string, string[]>()
  for (const row of rows) {
    if (!row.deviceId) continue
    const list = out.get(row.deviceId) ?? []
    if (list.length < PEERS_MAX) list.push(row.endpointId)
    out.set(row.deviceId, list)
  }
  return out
}

function online(row: DeviceRow, now: Date): boolean {
  return !!row.lastSeenAt && now.getTime() - row.lastSeenAt.getTime() <= contract.device.onlineWindowSeconds * 1000
}

export async function startTransfer(
  db: Db,
  args: {
    userId: string
    sessionId?: string | null
    /** #EXP-1440: the asking session's team; a machine shared only with
     *  OTHER teams is theirs (unset = the person themselves, no session). */
    teamId?: string | null
    fromDeviceId: string
    toDeviceId: string
    srcPath: string
    destPath: string
    now?: Date
  },
  nudge: (userId: string, deviceId: string) => void = () => {}
) {
  if (args.fromDeviceId === args.toDeviceId) throw new TRPCError({ code: `BAD_REQUEST`, message: TRANSFER_COPY.sameDevice })
  for (const [name, path] of [[`srcPath`, args.srcPath], [`destPath`, args.destPath]] as const) {
    const error = transferPathError(path)
    if (error) throw new TRPCError({ code: `BAD_REQUEST`, message: `${name}: ${error}` })
  }
  // The same-person rule: both rows are the CALLER's own machines. A
  // teammate's shared server is never a side (its row is the teammate's).
  const rows: DeviceRow[] = await db
    .select({
      id: devices.id,
      deviceId: devices.deviceId,
      label: devices.label,
      caps: devices.caps,
      lastSeenAt: devices.lastSeenAt,
      sharedTeamIds: devices.sharedTeamIds,
    })
    .from(devices)
    .where(and(eq(devices.userId, args.userId), inArray(devices.deviceId, [args.fromDeviceId, args.toDeviceId])))
  const from = rows.find((row) => row.deviceId === args.fromDeviceId)
  const to = rows.find((row) => row.deviceId === args.toDeviceId)
  if (!from || !to) throw new TRPCError({ code: `NOT_FOUND`, message: TRANSFER_COPY.notYours })
  const teamId = args.teamId
  if (teamId && ![from, to].every((row) => deviceUsableInTeam(row, teamId))) {
    // #EXP-1720: the owner's personal team (their Assistant) reaches any machine of theirs.
    if (teamId !== (await getPersonalTeamId(args.userId))) {
      throw new TRPCError({ code: `FORBIDDEN`, message: DEVICE_OTHER_TEAM_MESSAGE })
    }
  }
  for (const row of [from, to]) {
    if (!(row.caps ?? []).includes(FILE_TRANSFER_CAP)) {
      throw new TRPCError({ code: `PRECONDITION_FAILED`, message: TRANSFER_COPY.noCap(row.label) })
    }
  }
  const endpoints = await liveEndpoints(db, args.userId, [from.id, to.id])
  const fromPeers = endpoints.get(from.id) ?? []
  const toPeers = endpoints.get(to.id) ?? []
  if (fromPeers.length === 0) throw new TRPCError({ code: `PRECONDITION_FAILED`, message: TRANSFER_COPY.noInstall(from.label) })
  if (toPeers.length === 0) throw new TRPCError({ code: `PRECONDITION_FAILED`, message: TRANSFER_COPY.noInstall(to.label) })

  const [live] = await db
    .select({ n: count() })
    .from(deviceTransfers)
    .where(and(eq(deviceTransfers.userId, args.userId), notInArray(deviceTransfers.status, [...TRANSFER_TERMINAL])))
  if (Number(live?.n ?? 0) >= TRANSFERS_LIVE_MAX) throw new TRPCError({ code: `TOO_MANY_REQUESTS`, message: TRANSFER_COPY.tooMany })

  const [row] = await db
    .insert(deviceTransfers)
    .values({
      userId: args.userId,
      sessionId: args.sessionId ?? null,
      fromDeviceId: from.deviceId,
      toDeviceId: to.deviceId,
      srcPath: args.srcPath,
      destPath: args.destPath,
    })
    .returning()
  // `peers` is the one list in a command payload (the column is typed for
  // the string-only kinds; the machine reads it as a JSON array).
  const payload = (value: Record<string, string | string[]>) => value as unknown as Record<string, string>
  await db.insert(deviceCommands).values([
    {
      deviceRowId: from.id,
      userId: args.userId,
      kind: FILE_SEND_KIND,
      payload: payload({ transferId: row.id, srcPath: args.srcPath, peerDeviceId: to.deviceId, peers: toPeers }),
    },
    {
      deviceRowId: to.id,
      userId: args.userId,
      kind: FILE_RECEIVE_KIND,
      payload: payload({ transferId: row.id, destPath: args.destPath, peerDeviceId: from.deviceId, peers: fromPeers }),
    },
  ])
  nudge(args.userId, from.deviceId)
  nudge(args.userId, to.deviceId)
  const now = args.now ?? new Date()
  return { ...transferView(row), fromOnline: online(from, now), toOnline: online(to, now) }
}

export async function getTransfer(db: Db, userId: string, transferId: string): Promise<DeviceTransfer> {
  const [row] = await db
    .select()
    .from(deviceTransfers)
    .where(and(eq(deviceTransfers.id, transferId), eq(deviceTransfers.userId, userId)))
    .limit(1)
  if (!row) throw new TRPCError({ code: `NOT_FOUND`, message: TRANSFER_COPY.notFound })
  return row
}

export async function listTransfers(db: Db, userId: string, limit: number): Promise<DeviceTransfer[]> {
  return db
    .select()
    .from(deviceTransfers)
    .where(eq(deviceTransfers.userId, userId))
    .orderBy(desc(deviceTransfers.createdAt))
    .limit(limit)
}

export interface TransferReport {
  transferId: string
  role: `send` | `receive`
  status: `running` | `done` | `failed`
  files?: number
  filesDone?: number
  bytes?: number
  bytesDone?: number
  skipped?: string[]
  error?: string
}

/** A machine's progress or outcome. Terminal once: a report on an ended
 *  transfer changes nothing and answers its status (the machine stops). */
export async function reportTransfer(db: typeof defaultDb, userId: string, input: TransferReport) {
  return db.transaction(async (tx) => {
    const [row] = await tx
      .select()
      .from(deviceTransfers)
      .where(and(eq(deviceTransfers.id, input.transferId), eq(deviceTransfers.userId, userId)))
      .for(`update`)
    if (!row) throw new TRPCError({ code: `NOT_FOUND`, message: TRANSFER_COPY.notFound })
    if ((TRANSFER_TERMINAL as readonly string[]).includes(row.status)) return { ok: false, status: row.status }
    // Only the destination knows the copy is whole.
    const status = input.status === `done` && input.role !== `receive` ? `running` : input.status
    const terminal = status !== `running`
    const counters = {
      ...(input.files !== undefined ? { files: input.files } : {}),
      ...(input.filesDone !== undefined ? { filesDone: input.filesDone } : {}),
      ...(input.bytes !== undefined ? { bytes: input.bytes } : {}),
      ...(input.bytesDone !== undefined ? { bytesDone: input.bytesDone } : {}),
      ...(input.skipped ? { skipped: input.skipped.slice(0, SKIPPED_KEEP) } : {}),
    }
    const now = new Date()
    const [updated] = await tx
      .update(deviceTransfers)
      .set({
        // The destination's counters are the progress; the source's only count when it fails.
        ...(input.role === `receive` || terminal ? counters : {}),
        status,
        startedAt: row.startedAt ?? now,
        ...(terminal ? { completedAt: now, error: status === `failed` ? (input.error ?? `failed`).slice(0, 2000) : null } : {}),
        updatedAt: now,
      })
      .where(eq(deviceTransfers.id, row.id))
      .returning()
    if (terminal) await endTransferInTx(tx, updated)
    else {
      // An outside host's task on it moves (tasks/get reads the counters live).
      await tx
        .update(mcpTasks)
        .set({ lastUpdatedAt: now })
        .where(and(eq(mcpTasks.kind, `transfer`), eq(mcpTasks.ref, row.id), inArray(mcpTasks.status, [`working`, `input_required`])))
    }
    return { ok: true, status }
  })
}

export async function cancelTransfer(db: typeof defaultDb, userId: string, transferId: string) {
  return db.transaction(async (tx) => {
    const [row] = await tx
      .select()
      .from(deviceTransfers)
      .where(and(eq(deviceTransfers.id, transferId), eq(deviceTransfers.userId, userId)))
      .for(`update`)
    if (!row) throw new TRPCError({ code: `NOT_FOUND`, message: TRANSFER_COPY.notFound })
    if ((TRANSFER_TERMINAL as readonly string[]).includes(row.status)) return transferView(row)
    const now = new Date()
    const [updated] = await tx
      .update(deviceTransfers)
      .set({ status: `cancelled`, completedAt: now, updatedAt: now })
      .where(eq(deviceTransfers.id, row.id))
      .returning()
    await endTransferInTx(tx, updated)
    return transferView(updated)
  })
}

/** An ended transfer: its still-pending commands close with it (`done` with
 *  a done transfer, else `failed`: an offline machine back later does
 *  nothing), and the asking session wakes on one line. */
async function endTransferInTx(tx: Tx, row: DeviceTransfer): Promise<void> {
  await tx
    .update(deviceCommands)
    .set({ status: row.status === `done` ? `done` : `failed`, result: `Transfer ${row.status}.`, completedAt: new Date() })
    .where(
      and(
        eq(deviceCommands.userId, row.userId),
        inArray(deviceCommands.kind, [FILE_SEND_KIND, FILE_RECEIVE_KIND]),
        eq(deviceCommands.status, `pending`),
        sql`${deviceCommands.payload}->>'transferId' = ${row.id}`
      )
    )
  const labels = await tx
    .select({ deviceId: devices.deviceId, label: devices.label })
    .from(devices)
    .where(and(eq(devices.userId, row.userId), inArray(devices.deviceId, [row.fromDeviceId, row.toDeviceId])))
  const label = (id: string) => labels.find((entry) => entry.deviceId === id)?.label ?? id
  const text = transferEndText(row, { from: label(row.fromDeviceId), to: label(row.toDeviceId) })
  // An outside host's MCP task (emcp tasks.json TRANSFERS) ends with it.
  await endTransferTasks(tx, row.id, { status: row.status as `done` | `failed` | `cancelled`, text })
  if (!row.sessionId) return
  const [session] = await tx
    .select({ id: sessions.id })
    .from(sessions)
    .where(and(eq(sessions.id, row.sessionId), ne(sessions.status, `ended`)))
    .limit(1)
  if (!session) return
  // Pending input, like a fired watch: the session wakes and reads it.
  await insertMessage(tx, session.id, {
    authorKind: `user`,
    authorId: row.userId,
    body: text,
    card: { kind: `event`, event: `watch_fired`, sessionId: session.id, watchId: row.id },
  })
}
