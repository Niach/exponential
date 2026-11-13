// #FEED-244 (part D of #FEED-218): `exponential_apps {op: "screenshot"}`,
// one app screen as a PICTURE an agent can look at and show. The web image
// has no browser, so the server never paints: it resolves the screen
// (`resolveAppScreen`, the viewer's data, the owner-only Run as), mints a
// signed upload for a LOOSE attachment in the app's team, and queues an
// `app_screenshot` device command on the CALLING SESSION's device. The
// device (CLI daemon or desktop app, `updater::app_shot`) paints the bundle
// headless with the eUI gpui painter (`exponential-shot`, crates/eui-shot)
// and PUTs the PNG; the server finalizes the attachment as the caller.
import { and, eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { devices, sessions } from "@/db/schema"
import {
  assertLooseAttachmentUploadAccess,
  finalizeSignedAttachmentUpload,
  mintSignedAttachmentUpload,
  type FinalizedAttachmentResult,
} from "@/lib/emcp/handlers/attachments-upload"
import type { McpAccess } from "@/lib/emcp/scope"
import { sessionDeviceOwner } from "@/lib/sessions/host"
import type { RenderFormFactor, ResolvedAppScreen } from "./render"
import { awaitDeviceCommand, queueDeviceCommand } from "@/lib/trpc/devices"

/** The device command kind (`updater::app_shot::COMMAND_KIND`). */
export const APP_SCREENSHOT_COMMAND = `app_screenshot`
/** The heartbeat cap of a device that answers it (`updater::app_shot::CAP`). */
export const APP_SCREENSHOT_CAP = `app-screenshot`
/** The bundle format `exponential-shot` reads (`eui_shot::BUNDLE_SCHEMA`). */
export const SHOT_BUNDLE_SCHEMA = 1
/** The frame per form factor (CSS px; the preview's phone width). */
export const SCREENSHOT_FRAMES: Record<RenderFormFactor, { width: number; height: number }> = {
  phone: { width: 390, height: 844 },
  desktop: { width: 1280, height: 800 },
}
export type ScreenshotFormFactor = RenderFormFactor
/** How long the call waits for the device (its first shot fetches the renderer). */
export const SCREENSHOT_WAIT_MS = 60_000
/** A device seen this recently is online enough to ask. */
const ONLINE_MS = 2 * 60_000

/** What `resolveAppScreen` answers that a picture needs. */
export type ResolvedScreenForShot = Pick<ResolvedAppScreen, `pkg` | `templateId` | `data`> & {
  app: Pick<ResolvedAppScreen[`app`], `id` | `teamId` | `slug`>
}

/** The bundle `exponential-shot` paints (eui-shot's `parse_bundle`). */
export function screenshotBundle(resolved: ResolvedScreenForShot, formFactor: ScreenshotFormFactor) {
  const frame = SCREENSHOT_FRAMES[formFactor]
  return {
    schema: SHOT_BUNDLE_SCHEMA,
    package: resolved.pkg,
    templateId: resolved.templateId,
    data: resolved.data,
    width: frame.width,
    height: frame.height,
    mode: `dark`,
  }
}

/** The device row a screenshot runs on: the calling session's own device.
 *  Every refusal is a sentence the agent can act on. */
export async function screenshotDevice(sessionId: string | null, now = new Date()) {
  if (!sessionId) {
    throw new Error(`screenshot paints on your session's device: call it from an Exponential session (desktop app or the exponential CLI daemon).`)
  }
  const [session] = await db
    .select({ deviceId: sessions.deviceId, userId: sessions.userId, teamId: sessions.teamId })
    .from(sessions)
    .where(eq(sessions.id, sessionId))
    .limit(1)
  if (!session) throw new Error(`Your session is gone.`)
  const hostUserId = await sessionDeviceOwner(session)
  const [row] = await db
    .select({ id: devices.id, name: devices.label, caps: devices.caps, lastSeenAt: devices.lastSeenAt })
    .from(devices)
    .where(and(eq(devices.deviceId, session.deviceId), eq(devices.userId, hostUserId)))
    .limit(1)
  return screenshotDeviceVerdict(row ?? null, session.deviceId, hostUserId, now)
}

/** Pure: the device row → the target, or the refusal. */
export function screenshotDeviceVerdict(
  row: { id: string; name: string | null; caps: string[] | null; lastSeenAt: Date | null } | null,
  deviceId: string,
  hostUserId: string,
  now: Date
) {
  if (!row) throw new Error(`Your session's device is not registered, so it cannot paint a screenshot.`)
  const name = row.name || `Your session's device`
  if (!(row.caps ?? []).includes(APP_SCREENSHOT_CAP)) {
    throw new Error(`${name} cannot paint app screenshots: update Exponential there (desktop app or CLI; the renderer runs on Linux x86_64/arm64 and Apple-silicon macOS).`)
  }
  if (!row.lastSeenAt || now.getTime() - row.lastSeenAt.getTime() > ONLINE_MS) {
    throw new Error(`${name} is offline, so it cannot paint a screenshot.`)
  }
  return { deviceRowId: row.id, deviceId, hostUserId, name }
}

/** Pure: the command's outcome → done, or the refusal (null = no answer in time). */
export function screenshotOutcome(outcome: { status: string; result: string | null } | null, name: string): void {
  if (!outcome) {
    throw new Error(`${name} did not answer within ${SCREENSHOT_WAIT_MS / 1000}s (its first screenshot downloads the renderer): try again in a minute.`)
  }
  if (outcome.status !== `done`) {
    throw new Error(`${name} could not paint the screen: ${outcome.result || `no reason given`}`)
  }
}

export interface AppScreenshotInput {
  userId: string
  access: McpAccess
  sessionId: string | null
  origin: string
  formFactor?: ScreenshotFormFactor
  /** The screen, resolved as the viewer (`resolveAppScreen`). */
  resolve: () => Promise<ResolvedScreenForShot>
}

export interface AppScreenshotDeps {
  device: typeof screenshotDevice
  queue: typeof queueDeviceCommand
  wait: (commandId: string) => ReturnType<typeof awaitDeviceCommand>
  gate: typeof assertLooseAttachmentUploadAccess
  mint: typeof mintSignedAttachmentUpload
  finalize: typeof finalizeSignedAttachmentUpload
}

const defaultDeps: AppScreenshotDeps = {
  device: screenshotDevice,
  queue: (database, args) => queueDeviceCommand(database, args),
  wait: (commandId) => awaitDeviceCommand(db, commandId, SCREENSHOT_WAIT_MS),
  gate: assertLooseAttachmentUploadAccess,
  mint: mintSignedAttachmentUpload,
  finalize: finalizeSignedAttachmentUpload,
}

/** The whole op: device → resolve → signed upload → command → wait → the attachment. */
export async function takeAppScreenshot(input: AppScreenshotInput, deps: AppScreenshotDeps = defaultDeps): Promise<FinalizedAttachmentResult> {
  // The device first: a session without a capable, online device never
  // resolves anything.
  const target = await deps.device(input.sessionId)
  const resolved = await input.resolve()
  const formFactor = input.formFactor ?? `phone`
  // The picture is a loose file in the app's team: the grant covers it.
  await deps.gate({ teamId: resolved.app.teamId, userId: input.userId, access: input.access })
  const upload = await deps.mint({
    teamId: resolved.app.teamId,
    userId: input.userId,
    filename: `${resolved.app.slug}-${resolved.templateId}-${formFactor}.png`.replace(/[^A-Za-z0-9._-]/g, `-`),
    contentType: `image/png`,
    origin: input.origin,
  })
  const command = await deps.queue(db, {
    deviceRowId: target.deviceRowId,
    userId: target.hostUserId,
    deviceId: target.deviceId,
    kind: APP_SCREENSHOT_COMMAND,
    payload: { bundle: JSON.stringify(screenshotBundle(resolved, formFactor)), uploadUrl: upload.uploadUrl },
    onDuplicate: `reuse`,
  })
  screenshotOutcome(await deps.wait(command.id), target.name)
  return deps.finalize({ attachmentId: upload.attachmentId, userId: input.userId, access: input.access })
}
