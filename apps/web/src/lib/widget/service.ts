import { randomUUID } from "node:crypto"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray } from "drizzle-orm"
import { z } from "zod"
import { isPickableIcon, type PickableIcon } from "@exp/icons"
import { PICKABLE_ICON_SVG } from "@exp/icons/pickable-svg"
import { db } from "@/db/connection"
import {
  attachments,
  emailDeliveries,
  issues,
  issueLabels,
  issueSubscribers,
  boards,
  labels,
  widgetConfigs,
  widgetSubmissions,
  teams,
} from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { assertWithinStorageLimit } from "@/lib/billing"
import {
  MAX_REPORTER_MESSAGE_CHARS,
  reporterConversationUrl,
} from "@/lib/reporter/service"
import { mintReporterToken } from "@/lib/reporter/token"
import {
  escapeReporterText,
  titleFromReporterMessage,
} from "@/lib/reporter-text"
import { deliveryStatus, sendReporterConfirmationEmail } from "@/lib/email"
import { emailEnabled } from "@/lib/email-enabled"
import {
  buildAttachmentStorageKey,
  buildAttachmentUrl,
  isAcceptedImageContentType,
  maxImageUploadBytes,
  sanitizeUploadFilename,
} from "@/lib/storage/issue-attachments"
import { getImageDimensions } from "@/lib/storage/image-dimensions"
import { uploadObject, deleteObject } from "@/lib/storage"
import { getSoleHumanMemberId } from "@/lib/team-membership"
import { ensureSubscribed } from "@/lib/integrations/subscriptions"
import { recordIssueEvent } from "@/lib/integrations/activity"
import { fireAndForgetNewIssueNotify } from "@/lib/integrations/notifications"
import { buildWidgetDescription } from "./metadata"
import { isWidgetKeyFormat } from "./key"
import { isOriginAllowed } from "./origin"
import { corsHeaders, jsonResponse } from "./cors"
import { clientIpFromRequest, getWidgetConfigIpLimiter } from "./rate-limit"

// Screenshots are widget-generated (canvas encodes), so the accepted set is a
// deliberate subset of acceptedImageContentTypes — no gif/avif uploads here.
const screenshotContentTypes = new Set([
  `image/png`,
  `image/jpeg`,
  `image/webp`,
])

// Reporter-attached pictures (FEED-5). Unlike canvas-encoded screenshots
// these are arbitrary files, so they get the full image allowlist
// (isAcceptedImageContentType) instead of screenshotContentTypes.
export const maxWidgetImages = 3

// Screenshot + reporter pictures (10 MB each) + headroom for the multipart
// text fields.
export const maxSubmitRequestBytes =
  (1 + maxWidgetImages) * maxImageUploadBytes + 2 * 1024 * 1024

export class WidgetRequestError extends Error {
  constructor(
    readonly status: number,
    message: string,
    // Additive structured hint for the client. Email codes drive the panel's
    // email-recovery flow; `name_required` only maps to a friendlier form
    // error (names have no format validation, so no recovery machinery).
    readonly code?: `invalid_email` | `email_required` | `name_required`
  ) {
    super(message)
  }
}

// The widget config row plus the trash/archive state of its target board
// (SLOP-4: every widget has one — board_id is NOT NULL), so the submit +
// config paths can gate on live state.
export type WidgetConfigWithBoard = typeof widgetConfigs.$inferSelect & {
  boardSlug: string
  boardName: string
  boardDeletedAt: Date | null
  boardArchivedAt: Date | null
  teamSlug: string
  // The reporter-facing identity (REV2-51): the SAME name the conversation
  // page and every reporter email carry.
  teamName: string
}

export async function loadWidgetConfigByKey(
  key: string
): Promise<WidgetConfigWithBoard> {
  if (!isWidgetKeyFormat(key)) {
    throw new WidgetRequestError(404, `Unknown widget key`)
  }
  const [row] = await db
    .select({
      config: widgetConfigs,
      boardSlug: boards.slug,
      boardName: boards.name,
      boardDeletedAt: boards.deletedAt,
      boardArchivedAt: boards.archivedAt,
      teamSlug: teams.slug,
      teamName: teams.name,
    })
    .from(widgetConfigs)
    .innerJoin(boards, eq(boards.id, widgetConfigs.boardId))
    .innerJoin(teams, eq(teams.id, widgetConfigs.teamId))
    .where(eq(widgetConfigs.publicKey, key))
    .limit(1)
  if (!row) {
    throw new WidgetRequestError(404, `Unknown widget key`)
  }
  return {
    ...row.config,
    boardSlug: row.boardSlug,
    boardName: row.boardName,
    boardDeletedAt: row.boardDeletedAt,
    boardArchivedAt: row.boardArchivedAt,
    teamSlug: row.teamSlug,
    teamName: row.teamName,
  }
}

// SLOP-4: the ONE availability gate — a trashed or archived target board
// rejects new writes (and hides the widget); restoring or unarchiving brings
// it back automatically.
export function widgetBoardAvailable(config: WidgetConfigWithBoard): boolean {
  return config.boardDeletedAt == null && config.boardArchivedAt == null
}

// Owner-defined extra inputs on the feedback form (EXP-244). Values are
// merged into the submitted customData blob client-side — no dedicated
// persistence. The write path validates shape (formConfigSchema in
// trpc/widgets.ts), but form_config is untyped jsonb, so the read paths
// re-sanitize defensively before anything ships to third-party pages.
export interface WidgetCustomFieldConfig {
  key: string
  label: string
  required: boolean
}

export const maxWidgetCustomFields = 8
export const widgetCustomFieldKeyPattern = /^[a-z0-9][a-z0-9_-]{0,39}$/

export function sanitizeWidgetCustomFields(
  formConfig: Record<string, unknown> | null | undefined
): WidgetCustomFieldConfig[] {
  const raw = formConfig?.customFields
  if (!Array.isArray(raw)) return []
  const out: WidgetCustomFieldConfig[] = []
  const seen = new Set<string>()
  for (const entry of raw) {
    if (out.length >= maxWidgetCustomFields) break
    if (entry === null || typeof entry !== `object`) continue
    const { key, label, required } = entry as Record<string, unknown>
    if (typeof key !== `string` || !widgetCustomFieldKeyPattern.test(key))
      continue
    if (seen.has(key)) continue
    if (typeof label !== `string`) continue
    const trimmedLabel = label.trim().slice(0, 40)
    if (trimmedLabel.length === 0) continue
    seen.add(key)
    out.push({ key, label: trimmedLabel, required: required === true })
  }
  return out
}

// Widget-exposed labels (EXP-435): form_config.labelIds names the team
// labels a visitor may tag their report with. Same defensive-read rule as
// custom fields — the column is untyped jsonb, so junk entries are dropped,
// and ids are resolved against live label rows at serve/submit time (labels
// can be deleted after the config write; stale ids must degrade, not 500).
export const maxWidgetLabels = 10

// Non-UUID strings would make inArray(labels.id, ...) raise 22P02 instead of
// degrading — the tRPC write schema enforces .uuid(), but this column is
// reachable by other writers.
const uuidShape =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

export function sanitizeWidgetLabelIds(
  formConfig: Record<string, unknown> | null | undefined
): string[] {
  const raw = formConfig?.labelIds
  if (!Array.isArray(raw)) return []
  const out: string[] = []
  for (const entry of raw) {
    if (out.length >= maxWidgetLabels) break
    if (typeof entry !== `string` || !uuidShape.test(entry)) continue
    if (out.includes(entry)) continue
    out.push(entry)
  }
  return out
}

export type WidgetThemePreference = `dark` | `light` | `auto`

export function sanitizeWidgetTheme(
  formConfig: Record<string, unknown> | null | undefined
): WidgetThemePreference | null {
  const raw = formConfig?.theme
  return raw === `dark` || raw === `light` || raw === `auto` ? raw : null
}

export function sanitizeWidgetHexColor(value: unknown): string | null {
  return typeof value === `string` && /^#[0-9a-fA-F]{6}$/.test(value)
    ? value
    : null
}

// Launcher appearance (EXP-569): per-device mode/position plus an optional
// pickable icon. Values are shared verbatim with packages/widget's launcher
// resolution — the widget re-validates everything it receives.
export const widgetLauncherModes = [`fab`, `tab`] as const
export const widgetLauncherPositions = [
  `top-left`,
  `top-right`,
  `middle-left`,
  `middle-right`,
  `bottom-left`,
  `bottom-right`,
] as const
export type WidgetLauncherMode = (typeof widgetLauncherModes)[number]
export type WidgetLauncherPosition = (typeof widgetLauncherPositions)[number]
export interface WidgetLauncherPlacement {
  mode: WidgetLauncherMode
  position: WidgetLauncherPosition
}

// Must stay byte-equal to packages/widget's defaultLauncher — a mismatch
// makes the launcher jump when the config fetch resolves.
export const defaultWidgetLauncher: Record<
  `desktop` | `mobile`,
  WidgetLauncherPlacement
> = {
  desktop: { mode: `fab`, position: `bottom-right` },
  mobile: { mode: `tab`, position: `middle-right` },
}

export interface WidgetLauncherConfig {
  desktop: WidgetLauncherPlacement
  mobile: WidgetLauncherPlacement
  icon: PickableIcon | null
}

// The EFFECTIVE launcher a row resolves to. Precedence per device: a stored
// `launcher` entry > the defaults. Same defensive-read rule as every sibling
// sanitizer. (EXP-672: the pre-EXP-569 two-value `position` read shim is
// gone — every stored row carries `launcher`.)
export function sanitizeWidgetLauncher(
  formConfig: Record<string, unknown> | null | undefined
): WidgetLauncherConfig {
  const raw =
    formConfig?.launcher !== null && typeof formConfig?.launcher === `object`
      ? (formConfig.launcher as Record<string, unknown>)
      : {}
  const device = (key: `desktop` | `mobile`): WidgetLauncherPlacement => {
    const entry = raw[key]
    if (entry !== null && typeof entry === `object`) {
      const { mode, position } = entry as Record<string, unknown>
      if (
        (widgetLauncherModes as readonly unknown[]).includes(mode) &&
        (widgetLauncherPositions as readonly unknown[]).includes(position)
      ) {
        return { mode, position } as WidgetLauncherPlacement
      }
    }
    return defaultWidgetLauncher[key]
  }
  return {
    desktop: device(`desktop`),
    mobile: device(`mobile`),
    icon:
      typeof raw.icon === `string` && isPickableIcon(raw.icon)
        ? raw.icon
        : null,
  }
}

// The configured labels resolved to live team rows, in the team's label
// order — what the config route serves so the panel can render name+color.
export async function resolveWidgetConfigLabels(
  config: WidgetConfigWithBoard
): Promise<{ id: string; name: string; color: string }[]> {
  const labelIds = sanitizeWidgetLabelIds(config.formConfig)
  if (labelIds.length === 0) return []
  return await db
    .select({ id: labels.id, name: labels.name, color: labels.color })
    .from(labels)
    .where(and(inArray(labels.id, labelIds), eq(labels.teamId, config.teamId)))
    .orderBy(asc(labels.sortOrder))
}

// The ONE normalized view of the EXP-244 email/name toggle bag — served by
// the config route AND enforced by the submit paths, so the form a visitor
// sees can never disagree with the policy the server enforces (a raw
// `{nameRequired: true}` row without collectName — writable via a direct
// tRPC call — used to serve no name field while 400ing every submit).
// Required always implies collect for email; a hidden name field is never
// required.
export function normalizedWidgetFormToggles(
  formConfig: Record<string, unknown> | null | undefined
): {
  emailRequired: boolean
  collectEmail: boolean
  collectName: boolean
  nameRequired: boolean
} {
  const form = formConfig ?? {}
  const collectName = form.collectName === true
  return {
    emailRequired: form.emailRequired === true,
    collectEmail:
      form.emailRequired === true ? true : form.collectEmail !== false,
    collectName,
    nameRequired: collectName && form.nameRequired === true,
  }
}

// SLOP-4: ONE submit shape. `message` is the reporter's text; a host may
// pass a `title` (the loader's submit API), else it derives from the first
// line. Both land as escaped reporter text.
// issues.title is varchar(500), and escaping can double a title's length,
// so the cap lands AFTER the escape (like titleFromReporterMessage). A cut
// never leaves a dangling backslash in front of the ellipsis.
const ISSUE_TITLE_MAX = 500

function legacyReporterTitle(raw: string): string {
  const escaped = escapeReporterText(raw)
  if (escaped.length <= ISSUE_TITLE_MAX) return escaped
  let cut = escaped.slice(0, ISSUE_TITLE_MAX - 1).trimEnd()
  const trailing = cut.length - cut.replace(/\\+$/, ``).length
  if (trailing % 2 === 1) cut = cut.slice(0, -1)
  return `${cut}…`
}

const submitFieldsSchema = z.object({
  message: z.string().max(MAX_REPORTER_MESSAGE_CHARS).default(``),
  title: z.string().trim().max(500).default(``),
  email: z
    .string()
    .trim()
    .email()
    .max(320)
    .optional()
    .or(z.literal(``).transform(() => undefined)),
  name: z.string().trim().max(255).optional(),
  userId: z.string().trim().max(255).optional(),
})

const envMetaSchema = z
  .object({
    url: z.string().max(4096).optional(),
    viewportWidth: z.coerce.number().int().min(0).max(100_000).optional(),
    viewportHeight: z.coerce.number().int().min(0).max(100_000).optional(),
    screenWidth: z.coerce.number().int().min(0).max(100_000).optional(),
    screenHeight: z.coerce.number().int().min(0).max(100_000).optional(),
    devicePixelRatio: z.coerce.number().min(0).max(100).optional(),
  })
  .partial()

function parseJsonField(
  raw: FormDataEntryValue | null,
  maxChars: number,
  label: string
): Record<string, unknown> | null {
  if (typeof raw !== `string` || raw.length === 0) return null
  if (raw.length > maxChars) {
    throw new WidgetRequestError(400, `${label} too large`)
  }
  try {
    const parsed: unknown = JSON.parse(raw)
    if (
      parsed === null ||
      typeof parsed !== `object` ||
      Array.isArray(parsed)
    ) {
      throw new Error(`not an object`)
    }
    return parsed as Record<string, unknown>
  } catch {
    throw new WidgetRequestError(400, `Invalid ${label}`)
  }
}

// The reporter's label picks (EXP-435): a JSON array of label ids. Malformed
// payloads are a client bug and 400; ids OUTSIDE the configured set are
// silently dropped — the widget caches its config for 5 minutes, so a label
// the owner just removed must not fail the whole report.
function parseWidgetLabelSelection(
  raw: FormDataEntryValue | null,
  formConfig: Record<string, unknown> | null | undefined
): string[] {
  if (raw === null || raw === ``) return []
  if (typeof raw !== `string` || raw.length > 2 * 1024) {
    throw new WidgetRequestError(400, `Invalid labels`)
  }
  let parsed: unknown
  try {
    parsed = JSON.parse(raw)
  } catch {
    throw new WidgetRequestError(400, `Invalid labels`)
  }
  if (
    !Array.isArray(parsed) ||
    parsed.some((entry) => typeof entry !== `string`)
  ) {
    throw new WidgetRequestError(400, `Invalid labels`)
  }
  const configured = sanitizeWidgetLabelIds(formConfig)
  return [...new Set(parsed as string[])]
    .filter((id) => configured.includes(id))
    .slice(0, maxWidgetLabels)
}

export interface WidgetSubmitResult {
  // The created issue. `url` is always null since public boards were removed
  // (EXP-180) — the field survives because cached third-party widget bundles
  // read it.
  issueId: string
  identifier: string
  url: null
  // When the reporter gave an email (REV2-10): did the confirmation email
  // carrying their magic link — their ONLY way back into the conversation —
  // actually go out? `null` when no email was given (nothing to mail). The
  // panel degrades its success copy honestly on false; the link is never
  // returned inline (it is a credential, and this response is anonymous).
  emailDelivered: boolean | null
}

// The whole submit pipeline past key/origin/rate gating (which the route owns
// because those decide the CORS headers on the response).
export async function createWidgetSubmission(args: {
  config: WidgetConfigWithBoard
  formData: FormData
  userAgent: string | null
}): Promise<WidgetSubmitResult> {
  const { config, formData } = args

  // A trashed or archived target board rejects new writes; restoring or
  // unarchiving brings a hidden board back automatically.
  const boardId = config.boardId
  if (!widgetBoardAvailable(config)) {
    throw new WidgetRequestError(403, `This feedback board is unavailable`)
  }

  const fields = submitFieldsSchema.safeParse({
    message: formData.get(`message`) ?? ``,
    title: formData.get(`title`) ?? ``,
    email: formData.get(`email`) ?? undefined,
    name: formData.get(`name`) ?? undefined,
    userId: formData.get(`userId`) ?? undefined,
  })
  if (!fields.success) {
    // Flag an implicated email so the client can re-reveal its email input
    // instead of surfacing a generic failure over a hidden identity address.
    const emailIssue = fields.error.issues.some(
      (issue) => issue.path[0] === `email`
    )
    throw new WidgetRequestError(
      400,
      `Invalid submission fields`,
      emailIssue ? `invalid_email` : undefined
    )
  }

  // The reporter's words — UNTRUSTED plain text, escaped ONCE into GFM that
  // renders literally on every client (lib/reporter-text.ts). A report needs
  // SOME text (a title or a message) to file as an issue.
  const reporterText = fields.data.message.trim()
  if (!fields.data.title && !reporterText) {
    throw new WidgetRequestError(400, `Invalid submission fields`)
  }
  const title = fields.data.title
    ? legacyReporterTitle(fields.data.title)
    : titleFromReporterMessage(reporterText)

  // The panel's required-email gate is advisory only — it vanishes when the
  // config fetch loses the race with the first open (or fails), and raw POSTs
  // never see it. Enforce the board owner's policy here so every report on a
  // required-email board stays contactable via the resolution email. Always
  // the NORMALIZED toggle view the config route serves, never the raw bag.
  const toggles = normalizedWidgetFormToggles(config.formConfig)
  if (toggles.emailRequired && !fields.data.email) {
    throw new WidgetRequestError(400, `Email is required`, `email_required`)
  }

  // Same advisory-gate rule for the name toggle (EXP-244): the panel's
  // required marker can lag the config (5-min cache) or be absent on cached
  // pre-name bundles, so the owner's policy is enforced here.
  if (toggles.nameRequired && !fields.data.name) {
    throw new WidgetRequestError(400, `Name is required`, `name_required`)
  }

  const customData = parseJsonField(
    formData.get(`customData`),
    8 * 1024,
    `customData`
  )

  // Required custom fields (EXP-244): values ride the merged customData blob,
  // so requiredness is checked against it — a host-provided setCustomData
  // value satisfies the field just like a typed one.
  for (const field of sanitizeWidgetCustomFields(config.formConfig)) {
    if (!field.required) continue
    // Own-property read only: the key pattern admits prototype names like
    // `constructor`, and an inherited function must never leak into the
    // presence check.
    const value =
      customData && Object.hasOwn(customData, field.key)
        ? customData[field.key]
        : undefined
    const present =
      typeof value === `number` ||
      typeof value === `boolean` ||
      (typeof value === `string` && value.trim().length > 0)
    if (!present) {
      throw new WidgetRequestError(400, `Please fill in "${field.label}"`)
    }
  }

  const labelIds = parseWidgetLabelSelection(
    formData.get(`labels`),
    config.formConfig
  )

  const metaRaw = parseJsonField(formData.get(`meta`), 4 * 1024, `meta`) ?? {}
  const meta = envMetaSchema.safeParse(metaRaw)
  if (!meta.success) {
    throw new WidgetRequestError(400, `Invalid meta`)
  }

  const screenshot = formData.get(`screenshot`)
  if (screenshot !== null && !(screenshot instanceof File)) {
    throw new WidgetRequestError(400, `Invalid screenshot`)
  }
  if (screenshot) {
    if (!screenshotContentTypes.has(screenshot.type)) {
      throw new WidgetRequestError(400, `Unsupported screenshot type`)
    }
    if (screenshot.size > maxImageUploadBytes) {
      throw new WidgetRequestError(413, `Screenshot too large`)
    }
  }

  // Reporter-attached pictures (FEED-5), alongside the optional screenshot.
  const imageEntries = formData.getAll(`images`)
  if (imageEntries.length > maxWidgetImages) {
    throw new WidgetRequestError(400, `Too many images`)
  }
  const images: File[] = []
  for (const entry of imageEntries) {
    if (!(entry instanceof File)) {
      throw new WidgetRequestError(400, `Invalid image`)
    }
    if (!isAcceptedImageContentType(entry.type)) {
      throw new WidgetRequestError(400, `Unsupported image type`)
    }
    if (entry.size > maxImageUploadBytes) {
      throw new WidgetRequestError(413, `Image too large`)
    }
    images.push(entry)
  }

  const uploadBytes =
    (screenshot?.size ?? 0) +
    images.reduce((total, image) => total + image.size, 0)
  try {
    await assertWithinStorageLimit(config.teamId, uploadBytes)
  } catch (error) {
    if (error instanceof TRPCError) {
      throw new WidgetRequestError(403, error.message)
    }
    throw error
  }

  // Pre-generate ids so the storage keys can use the issue id and the
  // description can embed the attachment URLs inside one transaction.
  // Screenshot first — the description embeds images in this order.
  const issueId = randomUUID()
  const uploads = [
    ...(screenshot
      ? [{ file: screenshot, fallbackFilename: `screenshot.png` }]
      : []),
    ...images.map((file) => ({ file, fallbackFilename: `image.png` })),
  ].map(({ file, fallbackFilename }) => {
    const attachmentId = randomUUID()
    const filename = sanitizeUploadFilename(file.name, fallbackFilename)
    return {
      file,
      attachmentId,
      filename,
      storageKey: buildAttachmentStorageKey(issueId, attachmentId, filename),
    }
  })
  const screenshotAttachmentId = screenshot
    ? (uploads[0]?.attachmentId ?? null)
    : null

  // EXP-42b: user text + images ONLY — reporter/page/env metadata stays in
  // the widget_submissions row below (members-only via widgets.submissionForIssue).
  const description = buildWidgetDescription({
    userText: escapeReporterText(reporterText),
    screenshotAttachmentId,
    imageAttachmentIds: uploads
      .filter((upload) => upload.attachmentId !== screenshotAttachmentId)
      .map((upload) => upload.attachmentId),
  })

  // EXP-50: a solo team (exactly one human member) auto-assigns widget
  // feedback to that member — there is nobody else it could belong to.
  const soleMemberId = await getSoleHumanMemberId(config.teamId)

  // S3 keys written so far — the catch below reclaims them when a later
  // upload or the transaction fails.
  const uploadedKeys: string[] = []
  try {
    const attachmentRows: {
      attachmentId: string
      filename: string
      contentType: string
      sizeBytes: number
      storageKey: string
      dimensions: { width: number; height: number } | null
    }[] = []
    for (const upload of uploads) {
      const body = new Uint8Array(await upload.file.arrayBuffer())
      await uploadObject({
        body,
        contentLength: upload.file.size,
        contentType: upload.file.type,
        key: upload.storageKey,
      })
      uploadedKeys.push(upload.storageKey)
      attachmentRows.push({
        attachmentId: upload.attachmentId,
        filename: upload.filename,
        contentType: upload.file.type,
        sizeBytes: upload.file.size,
        storageKey: upload.storageKey,
        dimensions: getImageDimensions(body),
      })
    }

    // Direct insert with the attachment row in the SAME transaction: the
    // tRPC create's "no images at create time" rule exists because client
    // uploads happen after create — here the attachment exists before commit,
    // so the embedded image URL is valid the moment the issue is visible.
    // The issue has NO user creator (creator_id null, source `widget`) — there
    // is no synthetic bot; clients key the "Feedback widget" origin off source.
    // Member fan-out happens AFTER commit via fireAndForgetNewIssueNotify
    // (EXP-53) — every human team member gets an `issue_created` notification.
    const result = await db.transaction(async (tx) => {
      await generateTxId(tx)
      const [issue] = await tx
        .insert(issues)
        .values({
          id: issueId,
          boardId,
          // populate_issue_board_context overwrites with board-derived
          // truth; passed to satisfy the NOT NULL insert contract.
          teamId: config.teamId,
          title,
          status: `backlog`,
          priority: `none`,
          // Post-EXP-42b a text-less, screenshot-less submission has an empty
          // description — store null like the tRPC mutations do.
          description: description || null,
          assigneeId: soleMemberId,
          creatorId: null,
          source: `widget`,
        })
        .returning({
          id: issues.id,
          identifier: issues.identifier,
          status: issues.status,
          statusId: issues.statusId,
          priority: issues.priority,
        })

      // EXP-530: `created` event (timeline-suppressed; feeds automation event
      // triggers). No actor — the reporter is anonymous, like the issue.
      await recordIssueEvent(tx, {
        issueId,
        teamId: config.teamId,
        actorUserId: null,
        type: `created`,
        payload: {
          status: issue.status,
          statusId: issue.statusId,
          priority: issue.priority,
          source: `widget`,
        },
      })

      // Reporter-picked labels (EXP-435). Re-selected against live team rows
      // inside the tx — the configured set can hold ids deleted since the
      // config write, and those must silently drop. No actor: like the issue
      // itself, the labels come from the anonymous reporter.
      if (labelIds.length > 0) {
        const labelRows = await tx
          .select({ id: labels.id })
          .from(labels)
          .where(
            and(inArray(labels.id, labelIds), eq(labels.teamId, config.teamId))
          )
        if (labelRows.length > 0) {
          await tx
            .insert(issueLabels)
            .values(
              labelRows.map(({ id }) => ({
                issueId,
                labelId: id,
                teamId: config.teamId,
                boardId,
              }))
            )
            .onConflictDoNothing()
        }
      }

      for (const row of attachmentRows) {
        await tx.insert(attachments).values({
          id: row.attachmentId,
          teamId: config.teamId,
          boardId,
          issueId,
          // No synthetic uploader — widget images have a null uploader.
          uploaderId: null,
          filename: row.filename,
          contentType: row.contentType,
          sizeBytes: row.sizeBytes,
          storageKey: row.storageKey,
          url: buildAttachmentUrl(row.attachmentId),
          width: row.dimensions?.width ?? null,
          height: row.dimensions?.height ?? null,
        })
      }

      // EXP-50: subscribe the auto-assigned solo member like issues.create
      // subscribes explicit assignees. NO assignment notification for this —
      // the post-commit issue_created fan-out already reaches them, and a
      // second "assigned you" row would double-notify.
      if (soleMemberId) {
        await ensureSubscribed(tx, {
          issueId,
          userId: soleMemberId,
          teamId: config.teamId,
          source: `assignee`,
        })
      }

      // Record the external reporter as a `widget_reporter` subscriber (null
      // userId + email — no throwaway users row). They receive the clean
      // resolution email when the issue closes; member fan-out ignores these
      // rows (it filters on non-null userId).
      if (fields.data.email) {
        await tx.insert(issueSubscribers).values({
          issueId,
          userId: null,
          email: fields.data.email,
          teamId: config.teamId,
          boardId,
          source: `widget_reporter`,
          unsubscribed: false,
        })
      }

      await tx.insert(widgetSubmissions).values({
        widgetConfigId: config.id,
        issueId,
        reporterEmail: fields.data.email ?? null,
        reporterName: fields.data.name ?? null,
        reporterExternalId: fields.data.userId ?? null,
        pageUrl: meta.data.url ?? null,
        userAgent: args.userAgent,
        viewportWidth: meta.data.viewportWidth ?? null,
        viewportHeight: meta.data.viewportHeight ?? null,
        screenWidth: meta.data.screenWidth ?? null,
        screenHeight: meta.data.screenHeight ?? null,
        devicePixelRatio: meta.data.devicePixelRatio ?? null,
        customData,
      })

      return { issueId: issue.id, identifier: issue.identifier }
    })

    // EXP-53: after commit (the notification loads the issue row itself, so
    // it must be visible), fan out `issue_created` to the team's human
    // members. Fire-and-forget — never fails the submit.
    fireAndForgetNewIssueNotify({ issueId: result.issueId })

    // SLOP-4: a reporter who left an email gets the confirmation carrying
    // the magic conversation link (the issue's one stable reporter URL). A
    // failed send doesn't fail the (already committed) report, but it is NOT
    // invisible either (REV2-10): `emailDelivered` rides the submit response
    // so the panel can stop promising an email that never left. The ledger
    // row stores no URL — the token is never persisted, only recomputed per
    // email.
    const emailDelivered = fields.data.email
      ? await sendReporterConfirmation({
          to: fields.data.email,
          teamName: config.teamName,
          issueId: result.issueId,
          issueTitle: title,
        })
      : null

    return { ...result, url: null, emailDelivered }
  } catch (error) {
    for (const key of uploadedKeys) {
      try {
        await deleteObject(key)
      } catch (deleteError) {
        console.error(`Failed to rollback widget image object`, deleteError)
      }
    }
    throw error
  }
}

// The reporter's confirmation email + its ledger row. Never throws; returns
// whether the mail actually went out. With no transport nothing is sent or
// recorded.
async function sendReporterConfirmation(args: {
  to: string
  teamName: string
  issueId: string
  issueTitle: string
}): Promise<boolean> {
  if (!emailEnabled) return false
  try {
    const sendResult = await sendReporterConfirmationEmail({
      to: args.to,
      teamName: args.teamName,
      issueTitle: args.issueTitle,
      conversationUrl: reporterConversationUrl(mintReporterToken(args.issueId)),
    })
    await db.insert(emailDeliveries).values({
      userId: null,
      toEmail: args.to,
      issueId: args.issueId,
      kind: `reporter_confirmation`,
      status: deliveryStatus(sendResult),
      provider: sendResult.provider,
      providerMessageId: sendResult.messageId,
      subject: sendResult.subject,
      sentAt: sendResult.delivered ? new Date() : null,
    })
    return sendResult.delivered
  } catch (error) {
    console.error(`widget reporter confirmation email failed`, error)
    return false
  }
}

// The whole GET /api/widget/config pipeline lives here (not in the route
// file) so the route module's import surface stays identical to submit.ts —
// route files with a wider server-only import graph have failed to register
// under the nitro-alpha dev server (silent 404); see widget route files.
export async function handleWidgetConfig(request: Request): Promise<Response> {
  // Per-IP bucket BEFORE the key lookup (REV-25): the endpoint is anonymous
  // and the lookup joins three tables, so the domain allowlist — checked only
  // after the load — provides no load relief against a key-scanning loop.
  // Every sibling anonymous endpoint (widget submit, /api/support/*) takes a
  // bucket first; this one was the gap. The 429 echoes the requesting origin
  // permissively like the preflight handler — it carries no data, and the
  // real config response stays allowlist-gated.
  const ipLimit = getWidgetConfigIpLimiter().tryTake(
    `ip:${clientIpFromRequest(request)}`
  )
  if (!ipLimit.ok) {
    return jsonResponse(
      429,
      { error: `Too many requests, try again later` },
      {
        ...corsHeaders(request.headers.get(`origin`)),
        "Retry-After": String(ipLimit.retryAfterSeconds),
      }
    )
  }

  const key = new URL(request.url).searchParams.get(`key`) ?? ``

  let config
  try {
    config = await loadWidgetConfigByKey(key)
  } catch (error) {
    if (error instanceof WidgetRequestError) {
      return jsonResponse(error.status, { error: error.message })
    }
    console.error(`widget config error`, error)
    return jsonResponse(500, { error: `Internal error` })
  }

  const origin = isOriginAllowed(
    request.headers.get(`origin`),
    request.headers.get(`referer`),
    config.allowedDomains
  )
  if (!origin.allowed) {
    // No ACAO header: the browser blocks the response either way.
    return jsonResponse(403, { error: `Origin not allowed` })
  }

  const cors = corsHeaders(origin.echoOrigin)
  // Disabled, or its board is trashed/archived: nothing is servable.
  if (!config.enabled || !widgetBoardAvailable(config)) {
    return jsonResponse(200, { enabled: false }, cors)
  }

  const form = config.formConfig ?? {}
  // EXP-244 toggles — the same normalized view the submit paths enforce
  // (required always implies collect, a hidden name is never required), so
  // the served form and the enforced form can never disagree.
  const toggles = normalizedWidgetFormToggles(config.formConfig)
  return jsonResponse(
    200,
    {
      enabled: true,
      form: {
        buttonLabel:
          typeof form.buttonLabel === `string` ? form.buttonLabel : null,
        accentColor: sanitizeWidgetHexColor(form.accentColor),
        // EXP-569 — the resolved per-device launcher. `iconSvg` is
        // server-rendered markup from the shared icon registry, never stored
        // content. The legacy top-level `position` is no longer served
        // (EXP-672): every bundle since EXP-569 reads `launcher`, and the
        // bundle ships inside the same image as this server.
        launcher: (() => {
          const launcher = sanitizeWidgetLauncher(config.formConfig)
          return {
            desktop: launcher.desktop,
            mobile: launcher.mobile,
            iconSvg: launcher.icon ? PICKABLE_ICON_SVG[launcher.icon] : null,
          }
        })(),
        emailRequired: toggles.emailRequired,
        // ADDITIVE toggles — cached pre-fields bundles ignore them.
        collectEmail: toggles.collectEmail,
        collectName: toggles.collectName,
        nameRequired: toggles.nameRequired,
        customFields: sanitizeWidgetCustomFields(config.formConfig),
        // EXP-435 additions — cached pre-theme/pre-labels bundles ignore
        // them. Absent theme = dark (every pre-theme config). The EXP-435
        // backgroundColor/textColor overrides were removed by EXP-569.
        theme: sanitizeWidgetTheme(config.formConfig),
        labels: await resolveWidgetConfigLabels(config),
      },
      // maxImages/maxImageBytes are ADDITIVE (FEED-5) — cached pre-images
      // widget bundles ignore them.
      limits: {
        maxScreenshotBytes: maxImageUploadBytes,
        maxImageBytes: maxImageUploadBytes,
        maxImages: maxWidgetImages,
      },
    },
    { ...cors, "Cache-Control": `public, max-age=300` }
  )
}
