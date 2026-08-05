import { randomUUID } from "node:crypto"
import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { z } from "zod"
import { db } from "@/db/connection"
import {
  attachments,
  emailDeliveries,
  issues,
  issueSubscribers,
  boards,
  widgetConfigs,
  widgetSubmissions,
  teams,
} from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { assertCanUseHelpdesk, assertWithinStorageLimit } from "@/lib/billing"
import {
  createSupportThreadInTx,
  MAX_SUPPORT_MESSAGE_CHARS,
  supportThreadUrl,
  supportTicketTitle,
} from "@/lib/helpdesk/service"
import { deliveryStatus, sendSupportConfirmationEmail } from "@/lib/email"
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
import {
  fireAndForgetNewIssueNotify,
  fireAndForgetSupportThreadNotify,
} from "@/lib/integrations/notifications"
import { buildWidgetDescription } from "./metadata"
import { isWidgetKeyFormat } from "./key"
import { isOriginAllowed } from "./origin"
import { corsHeaders, jsonResponse } from "./cors"

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

// The widget config row plus the trash state of its feedback target
// board (nullable — support-only widgets have none) and the team's
// helpdesk flag, so the submit + config paths can gate each mode on live
// state.
export type WidgetConfigWithBoard = typeof widgetConfigs.$inferSelect & {
  boardSlug: string | null
  boardName: string | null
  boardDeletedAt: Date | null
  teamSlug: string | null
  // The reporter-facing helpdesk identity (REV2-51): the SAME name the
  // conversation page and every member reply email carry.
  teamName: string | null
  teamHelpdeskEnabled: boolean | null
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
      teamSlug: teams.slug,
      teamName: teams.name,
      teamHelpdeskEnabled: teams.helpdeskEnabled,
    })
    .from(widgetConfigs)
    .leftJoin(boards, eq(boards.id, widgetConfigs.boardId))
    .leftJoin(teams, eq(teams.id, widgetConfigs.teamId))
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
    teamSlug: row.teamSlug,
    teamName: row.teamName,
    teamHelpdeskEnabled: row.teamHelpdeskEnabled,
  }
}

// ---------------------------------------------------------------------------
// Widget modes: which entry points the panel offers. Stored on
// form_config.modes; absent = feedback-only (every pre-modes config).
// ---------------------------------------------------------------------------

export type WidgetMode = `feedback` | `support`

export function requestedWidgetModes(
  config: WidgetConfigWithBoard
): WidgetMode[] {
  const raw = config.formConfig?.modes
  const modes = Array.isArray(raw)
    ? [
        ...new Set(
          raw.filter(
            (mode): mode is WidgetMode =>
              mode === `feedback` || mode === `support`
          )
        ),
      ]
    : []
  return modes.length > 0 ? modes : [`feedback`]
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

// The ONE normalized view of the EXP-244 email/name toggle bag — served by
// the config route AND enforced by the submit paths, so the form a visitor
// sees can never disagree with the policy the server enforces (a raw
// `{nameRequired: true}` row without collectName — writable via a direct
// tRPC call — used to serve no name field while 400ing every submit).
// Required always implies collect for email; a hidden name field is never
// required. Support mode's email stays outside this: it is the reply channel
// and is unconditionally required by supportFieldsSchema.
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

// Support mode is served (and accepted) only while the TEAM helpdesk is
// on AND the plan still covers it — the owner-side write gate can go stale
// (helpdesk toggled off, plan lapsed), so both the config response and raw
// submits re-check dynamically.
async function widgetSupportAvailable(
  config: WidgetConfigWithBoard
): Promise<boolean> {
  if (config.teamHelpdeskEnabled !== true) return false
  try {
    await assertCanUseHelpdesk(config.teamId)
    return true
  } catch {
    return false
  }
}

// Per-mode availability: feedback needs a live target board, support the
// team helpdesk. An EMPTY result means nothing is servable — the config
// route reports the widget disabled and both submit paths 403.
export async function effectiveWidgetModes(
  config: WidgetConfigWithBoard
): Promise<WidgetMode[]> {
  const modes = requestedWidgetModes(config)
  const feedbackAvailable =
    config.boardId != null && config.boardDeletedAt == null
  const supportAvailable =
    modes.includes(`support`) && (await widgetSupportAvailable(config))

  const out: WidgetMode[] = []
  if (modes.includes(`feedback`) && feedbackAvailable) out.push(`feedback`)
  if (supportAvailable) out.push(`support`)
  return out
}

const submitFieldsSchema = z.object({
  title: z.string().trim().min(1).max(500),
  description: z.string().max(10_000).default(``),
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

export interface WidgetSubmitResult {
  // Feedback submissions carry the created issue; support submissions carry
  // neither (the ticket is a standalone thread and its conversation URL is
  // the reporter's emailed magic link). `url` is always null since public
  // boards were removed (EXP-180) — the field survives because cached
  // third-party widget bundles read it.
  issueId: string | null
  identifier: string | null
  url: null
  // Support submissions only (REV2-10): did the confirmation email carrying
  // the reporter's magic link — their ONLY way back into the conversation —
  // actually go out? `null` on feedback submissions, which mail nothing. The
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

  // A missing or trashed target board rejects new writes (a support-only
  // widget has no feedback board at all); restore brings a trashed board
  // back automatically.
  const boardId = config.boardId
  if (boardId == null || config.boardDeletedAt != null) {
    throw new WidgetRequestError(403, `This feedback board is unavailable`)
  }

  // A support-only widget must not accept feedback via raw POSTs — the UI
  // gate (which cards the panel offers) is advisory only.
  if (!(await effectiveWidgetModes(config)).includes(`feedback`)) {
    throw new WidgetRequestError(403, `Feedback is not enabled for this widget`)
  }

  const fields = submitFieldsSchema.safeParse({
    title: formData.get(`title`) ?? ``,
    description: formData.get(`description`) ?? ``,
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
    userText: fields.data.description,
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
          title: fields.data.title,
          status: `backlog`,
          priority: `none`,
          // Post-EXP-42b a text-less, screenshot-less submission has an empty
          // description — store null like the tRPC mutations do.
          description: description || null,
          assigneeId: soleMemberId,
          creatorId: null,
          source: `widget`,
        })
        .returning({ id: issues.id, identifier: issues.identifier })

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

      // One-way helpdesk (§6.4): record the external reporter as a
      // `widget_reporter` subscriber (null userId + email — no throwaway users
      // row). They receive the clean resolution email when the issue closes;
      // member fan-out ignores these rows (it filters on non-null userId).
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

      return {
        issueId: issue.id,
        identifier: issue.identifier,
        url: null,
        emailDelivered: null,
      }
    })

    // EXP-53: after commit (the notification loads the issue row itself, so
    // it must be visible), fan out `issue_created` to the team's human
    // members. Fire-and-forget — never fails the submit.
    fireAndForgetNewIssueNotify({ issueId: result.issueId })

    return result
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

// ---------------------------------------------------------------------------
// Support mode (EXP-130, reshaped by EXP-180): the widget's "Get help" form
// files a STANDALONE helpdesk ticket — a support thread + magic-link token,
// no issue — and the reporter gets a confirmation email carrying the
// conversation link. Tickets land in the team support inbox.
// ---------------------------------------------------------------------------

const supportFieldsSchema = z.object({
  message: z.string().trim().min(1).max(MAX_SUPPORT_MESSAGE_CHARS),
  // Required: the email is the reply channel — a support ticket without one
  // is a dead end.
  email: z.string().trim().email().max(320),
  name: z.string().trim().max(255).optional(),
  userId: z.string().trim().max(255).optional(),
})

export async function createWidgetSupportSubmission(args: {
  config: WidgetConfigWithBoard
  formData: FormData
  userAgent: string | null
}): Promise<WidgetSubmitResult> {
  const { config, formData } = args

  // Re-checked per submit (not just at config time): the team helpdesk
  // toggle or the plan may have changed since the widget cached its config.
  if (!(await effectiveWidgetModes(config)).includes(`support`)) {
    throw new WidgetRequestError(403, `Support is not enabled for this widget`)
  }

  const fields = supportFieldsSchema.safeParse({
    message: formData.get(`message`) ?? ``,
    email: formData.get(`email`) ?? ``,
    name: formData.get(`name`) ?? undefined,
    userId: formData.get(`userId`) ?? undefined,
  })
  if (!fields.success) {
    // A missing OR malformed support email both produce a path[0] === 'email'
    // issue — flag it so the client re-reveals its email input.
    const emailIssue = fields.error.issues.some(
      (issue) => issue.path[0] === `email`
    )
    throw new WidgetRequestError(
      400,
      `Invalid submission fields`,
      emailIssue ? `invalid_email` : undefined
    )
  }

  // Name toggle applies to the support form too (EXP-244) — the thread's
  // reporter_name is what the inbox shows. Same normalized view the config
  // route serves: a hidden name field is never required. (The email needs no
  // toggle here — support's reply channel is unconditionally required by the
  // schema above.)
  const toggles = normalizedWidgetFormToggles(config.formConfig)
  if (toggles.nameRequired && !fields.data.name) {
    throw new WidgetRequestError(400, `Name is required`, `name_required`)
  }

  const customData = parseJsonField(
    formData.get(`customData`),
    8 * 1024,
    `customData`
  )
  const metaRaw = parseJsonField(formData.get(`meta`), 4 * 1024, `meta`) ?? {}
  const meta = envMetaSchema.safeParse(metaRaw)
  if (!meta.success) {
    throw new WidgetRequestError(400, `Invalid meta`)
  }

  const { threadId, token } = await db.transaction(async (tx) => {
    await generateTxId(tx)
    const created = await createSupportThreadInTx(tx, {
      teamId: config.teamId,
      title: supportTicketTitle(fields.data.message),
      reporterEmail: fields.data.email,
      reporterName: fields.data.name ?? null,
      body: fields.data.message,
    })

    // Page/env context for the inbox details rail (widgets.submissionForThread).
    await tx.insert(widgetSubmissions).values({
      widgetConfigId: config.id,
      issueId: null,
      supportThreadId: created.threadId,
      reporterEmail: fields.data.email,
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

    return created
  })

  // Members learn of the new ticket through the support fan-out (inbox row +
  // push; email follows via the digest). Fire-and-forget — never fails the
  // submit.
  fireAndForgetSupportThreadNotify({ threadId, kind: `created` })

  // Confirmation email with the magic conversation link (the thread's one
  // stable URL). A failed send doesn't fail the (already committed) ticket,
  // but it is NOT invisible either (REV2-10): `emailDelivered` rides the
  // submit response so the panel can stop promising an email that never
  // left. The ledger row stores no thread URL — the token is never
  // persisted, only recomputed per email.
  let emailDelivered = false
  try {
    // ONE reporter-facing identity for the whole conversation (REV2-51): the
    // TEAM name — the same brand the /support/$token page shows and every
    // member reply email carries. (The email helper's parameter is still
    // called boardName.) Falls back to the widget's board/own name only when
    // the team row is somehow missing.
    const sendResult = await sendSupportConfirmationEmail({
      to: fields.data.email,
      boardName: config.teamName ?? config.boardName ?? config.name,
      threadUrl: supportThreadUrl(token),
    })
    emailDelivered = sendResult.delivered
    await db.insert(emailDeliveries).values({
      userId: null,
      toEmail: fields.data.email,
      issueId: null,
      kind: `support_confirmation`,
      status: deliveryStatus(sendResult),
      provider: sendResult.provider,
      providerMessageId: sendResult.messageId,
      sentAt: sendResult.delivered ? new Date() : null,
    })
  } catch (error) {
    console.error(`widget support confirmation email failed`, error)
  }

  // Support tickets carry no issue and no public URL — the magic-link page
  // is the reporter's view of the conversation.
  return { issueId: null, identifier: null, url: null, emailDelivered }
}

// The whole GET /api/widget/config pipeline lives here (not in the route
// file) so the route module's import surface stays identical to submit.ts —
// route files with a wider server-only import graph have failed to register
// under the nitro-alpha dev server (silent 404); see widget route files.
export async function handleWidgetConfig(request: Request): Promise<Response> {
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
  // Per-mode gating (EXP-162): a trashed feedback board no longer hides the
  // whole widget when a live split support target remains — the widget only
  // reports disabled when NOTHING is servable (or it's switched off).
  const modes = config.enabled ? await effectiveWidgetModes(config) : []
  if (modes.length === 0) {
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
      // Which entry points the panel offers (EXP-130). ADDITIVE — cached
      // pre-modes widget bundles ignore it and render feedback-only.
      modes,
      form: {
        buttonLabel:
          typeof form.buttonLabel === `string` ? form.buttonLabel : null,
        accentColor:
          typeof form.accentColor === `string` ? form.accentColor : null,
        // Default matches the loader's pre-config render (bottom-left) so the
        // launcher doesn't jump sides when the config fetch resolves.
        position:
          form.position === `bottom-right` ? `bottom-right` : `bottom-left`,
        emailRequired: toggles.emailRequired,
        // ADDITIVE toggles — cached pre-fields bundles ignore them.
        collectEmail: toggles.collectEmail,
        collectName: toggles.collectName,
        nameRequired: toggles.nameRequired,
        customFields: sanitizeWidgetCustomFields(config.formConfig),
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
