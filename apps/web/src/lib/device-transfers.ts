// #FEED-295: device-to-device file transfers between ONE person's own
// machines. The server only brokers: it checks that both machines are the
// caller's, that both run a build that can transfer (cap `file-transfer`)
// and have a live eNet install, logs the transfer (`device_transfers`), and
// queues one command per side: `file_send` on the source (serve `srcPath`
// to the destination's endpoint only) and `file_receive` on the destination
// (pull it into `destPath`). The bytes go machine to machine over eNet
// (QUIC, end-to-end encrypted, the relay as the fallback path), resumable,
// BLAKE3-checked per file: never through the server or its storage.
// Owner decision (Danny, 2026-11-23): no confirmation prompt; a transfer
// between the same person's own machines just runs. The machines still
// refuse unsafe paths themselves (crates/steer `file_transfer.rs`): `..`,
// symlink escapes, protected paths (~/.ssh, keyrings, credentials, the
// Exponential data dir) and an existing destination.

import { and, desc, eq, inArray, isNull, ne, sql } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { db as defaultDb } from "@/db/connection"
import { deviceCommands, devices, deviceTransfers, installs, sessions, type DeviceTransfer } from "@/db/schema"
import { contract } from "@exp/domain-contract"
import { insertMessage } from "@/lib/sessions/messages"
import type { Db } from "@/lib/sessions/tx"

export const FILE_TRANSFER_CAP = `file-transfer`
export const FILE_SEND_KIND = `file_send`
export const FILE_RECEIVE_KIND = `file_receive`
export const TRANSFER_TERMINAL = [`done`, `failed`, `cancelled`] as const
export const TRANSFER_PATH_MAX = 4096
const SKIPPED_KEEP = 200

export const TRANSFER_COPY = {
  sameDevice: `The source and the destination are the same device.`,
  notYours: `Both devices must be your own (exponential_devices list).`,
  noCap: (label: string) => `${label} runs a build without file transfers; update it first.`,
  noInstall: (label: string) => `${label} has no eNet node registered yet; start it once with this build.`,
  noDevice: `Name fromDeviceId and toDeviceId (one of them defaults to your session's device).`,
  notFound: `Transfer not found`,
} as const

/** A path a machine may resolve: absolute or `~`-relative, no `..`, no NUL.
 *  The machine applies the real rules (protected paths, symlinks). */
export function transferPathError(path: string): string | null {
  if (!path.trim()) return `The path is empty.`
  if (path.length > TRANSFER_PATH_MAX) return `The path is too long.`
  if (path.includes(`\0`)) return `The path contains a NUL byte.`
  const absolute = path.startsWith(`/`) || path === `~` || path.startsWith(`~/`) || /^[A-Za-z]:[\\/]/.test(path)
  if (!absolute) return `Use an absolute path or one starting with ~/.`
  if (path.split(/[\\/]+/).some((part) => part === `..`)) return `The path must not contain "..".`
  return null
}

export function formatBytes(bytes: number): string {
  const units = [`B`, `KB`, `MB`, `GB`, `TB`]
  let value = bytes
  let unit = 0
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000
    unit += 1
  }
  return unit === 0 ? `${bytes} B` : `${value.toFixed(value >= 100 ? 0 : 1)} ${units[unit]}`
}

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

type DeviceRow = { id: string; deviceId: string; label: string; caps: string[] | null; lastSeenAt: Date | null }

/** The newest live eNet install (desktop app or daemon) of each device row. */
async function liveEndpoints(db: Db, userId: string, rowIds: string[]): Promise<Map<string, string>> {
  const rows = await db
    .select({ deviceId: installs.deviceId, endpointId: installs.endpointId, lastSeenAt: installs.lastSeenAt })
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
  const out = new Map<string, string>()
  for (const row of rows) if (row.deviceId && !out.has(row.deviceId)) out.set(row.deviceId, row.endpointId)
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
  // The same-owner rule: both rows are the CALLER's own machines.
  const rows: DeviceRow[] = await db
    .select({ id: devices.id, deviceId: devices.deviceId, label: devices.label, caps: devices.caps, lastSeenAt: devices.lastSeenAt })
    .from(devices)
    .where(and(eq(devices.userId, args.userId), inArray(devices.deviceId, [args.fromDeviceId, args.toDeviceId])))
  const from = rows.find((row) => row.deviceId === args.fromDeviceId)
  const to = rows.find((row) => row.deviceId === args.toDeviceId)
  if (!from || !to) throw new TRPCError({ code: `NOT_FOUND`, message: TRANSFER_COPY.notYours })
  for (const row of [from, to]) {
    if (!(row.caps ?? []).includes(FILE_TRANSFER_CAP)) {
      throw new TRPCError({ code: `PRECONDITION_FAILED`, message: TRANSFER_COPY.noCap(row.label) })
    }
  }
  const endpoints = await liveEndpoints(db, args.userId, [from.id, to.id])
  const fromEndpoint = endpoints.get(from.id)
  const toEndpoint = endpoints.get(to.id)
  for (const [row, endpoint] of [[from, fromEndpoint], [to, toEndpoint]] as const) {
    if (!endpoint) throw new TRPCError({ code: `PRECONDITION_FAILED`, message: TRANSFER_COPY.noInstall(row.label) })
  }

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
  await db.insert(deviceCommands).values([
    {
      deviceRowId: from.id,
      userId: args.userId,
      kind: FILE_SEND_KIND,
      payload: { transferId: row.id, srcPath: args.srcPath, peer: toEndpoint!, peerDeviceId: to.deviceId },
    },
    {
      deviceRowId: to.id,
      userId: args.userId,
      kind: FILE_RECEIVE_KIND,
      payload: { transferId: row.id, destPath: args.destPath, peer: fromEndpoint!, peerDeviceId: from.deviceId },
    },
  ])
  nudge(args.userId, from.deviceId)
  nudge(args.userId, to.deviceId)
  const now = args.now ?? new Date()
  return {
    ...transferView(row),
    fromOnline: online(from, now),
    toOnline: online(to, now),
  }
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
    const [updated] = await tx
      .update(deviceTransfers)
      .set({
        ...(input.role === `receive` || terminal ? counters : {}),
        status,
        startedAt: row.startedAt ?? new Date(),
        ...(terminal ? { completedAt: new Date(), error: status === `failed` ? (input.error ?? `failed`).slice(0, 2000) : null } : {}),
        updatedAt: new Date(),
      })
      .where(eq(deviceTransfers.id, row.id))
      .returning()
    if (terminal) await endTransferInTx(tx, updated)
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
    const [updated] = await tx
      .update(deviceTransfers)
      .set({ status: `cancelled`, completedAt: new Date(), updatedAt: new Date() })
      .where(eq(deviceTransfers.id, row.id))
      .returning()
    await endTransferInTx(tx, updated)
    return transferView(updated)
  })
}

type Tx = Parameters<Parameters<typeof defaultDb.transaction>[0]>[0]

/** An ended transfer: its still-pending commands never run (an offline
 *  machine back later does nothing), and the asking session wakes. */
async function endTransferInTx(tx: Tx, row: DeviceTransfer): Promise<void> {
  await tx
    .update(deviceCommands)
    .set({ status: `failed`, result: `Transfer ${row.status}.`, completedAt: new Date() })
    .where(
      and(
        eq(deviceCommands.userId, row.userId),
        inArray(deviceCommands.kind, [FILE_SEND_KIND, FILE_RECEIVE_KIND]),
        eq(deviceCommands.status, `pending`),
        sql`${deviceCommands.payload}->>'transferId' = ${row.id}`
      )
    )
  if (!row.sessionId) return
  const [session] = await tx
    .select({ id: sessions.id })
    .from(sessions)
    .where(and(eq(sessions.id, row.sessionId), ne(sessions.status, `ended`)))
    .limit(1)
  if (!session) return
  const labels = await tx
    .select({ deviceId: devices.deviceId, label: devices.label })
    .from(devices)
    .where(and(eq(devices.userId, row.userId), inArray(devices.deviceId, [row.fromDeviceId, row.toDeviceId])))
  const label = (id: string) => labels.find((l) => l.deviceId === id)?.label ?? id
  // Pending input, like a fired watch: the session wakes and reads it.
  await insertMessage(tx, session.id, {
    authorKind: `user`,
    authorId: row.userId,
    body: transferEndText(row, { from: label(row.fromDeviceId), to: label(row.toDeviceId) }),
    card: { kind: `event`, event: `watch_fired`, sessionId: session.id, watchId: row.id },
  })
}
