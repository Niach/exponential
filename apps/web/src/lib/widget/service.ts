import { randomUUID } from "node:crypto"
import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { z } from "zod"
import { db } from "@/db/connection"
import {
  attachments,
  issues,
  issueSubscribers,
  projects,
  widgetConfigs,
  widgetSubmissions,
} from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { assertWithinStorageLimit } from "@/lib/billing"
import {
  buildAttachmentStorageKey,
  buildAttachmentUrl,
  maxImageUploadBytes,
} from "@/lib/storage/issue-attachments"
import { getImageDimensions } from "@/lib/storage/image-dimensions"
import { uploadObject, deleteObject } from "@/lib/storage"
import { buildWidgetDescription } from "./metadata"
import { isWidgetKeyFormat } from "./key"
import { isOriginAllowed } from "./origin"
import { corsHeaders, jsonResponse } from "./cors"

// Screenshots are widget-generated (canvas encodes), so the accepted set is a
// deliberate subset of acceptedImageContentTypes — no gif/avif uploads here.
const screenshotContentTypes = new Set([`image/png`, `image/jpeg`, `image/webp`])

// Screenshot cap (10 MB) + headroom for the multipart text fields.
export const maxSubmitRequestBytes = maxImageUploadBytes + 2 * 1024 * 1024

export class WidgetRequestError extends Error {
  constructor(
    readonly status: number,
    message: string
  ) {
    super(message)
  }
}

// The widget config row plus the trash/archive state of its target project, so
// the submit + config paths can treat a trashed board as unavailable.
export type WidgetConfigWithProject = typeof widgetConfigs.$inferSelect & {
  projectDeletedAt: Date | null
  projectArchivedAt: Date | null
}

export async function loadWidgetConfigByKey(
  key: string
): Promise<WidgetConfigWithProject> {
  if (!isWidgetKeyFormat(key)) {
    throw new WidgetRequestError(404, `Unknown widget key`)
  }
  const [row] = await db
    .select({
      config: widgetConfigs,
      projectDeletedAt: projects.deletedAt,
      projectArchivedAt: projects.archivedAt,
    })
    .from(widgetConfigs)
    .leftJoin(projects, eq(projects.id, widgetConfigs.projectId))
    .where(eq(widgetConfigs.publicKey, key))
    .limit(1)
  if (!row) {
    throw new WidgetRequestError(404, `Unknown widget key`)
  }
  return {
    ...row.config,
    projectDeletedAt: row.projectDeletedAt,
    projectArchivedAt: row.projectArchivedAt,
  }
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
  issueId: string
  identifier: string
}

// The whole submit pipeline past key/origin/rate gating (which the route owns
// because those decide the CORS headers on the response).
export async function createWidgetSubmission(args: {
  config: WidgetConfigWithProject
  formData: FormData
  userAgent: string | null
}): Promise<WidgetSubmitResult> {
  const { config, formData } = args

  // A trashed target board rejects new writes (its issues are unreachable and
  // the project may be purged); restore brings it back automatically.
  if (config.projectDeletedAt != null) {
    throw new WidgetRequestError(403, `This feedback board is unavailable`)
  }

  const fields = submitFieldsSchema.safeParse({
    title: formData.get(`title`) ?? ``,
    description: formData.get(`description`) ?? ``,
    email: formData.get(`email`) ?? undefined,
    name: formData.get(`name`) ?? undefined,
    userId: formData.get(`userId`) ?? undefined,
  })
  if (!fields.success) {
    throw new WidgetRequestError(400, `Invalid submission fields`)
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

  try {
    await assertWithinStorageLimit(config.workspaceId, screenshot?.size ?? 0)
  } catch (error) {
    if (error instanceof TRPCError) {
      throw new WidgetRequestError(403, error.message)
    }
    throw error
  }

  // Pre-generate ids so the storage key can use the issue id and the
  // description can embed the attachment URL inside one transaction.
  const issueId = randomUUID()
  const attachmentId = screenshot ? randomUUID() : null

  let storageKey: string | null = null
  let dimensions: { width: number; height: number } | null = null
  if (screenshot && attachmentId) {
    const filename = screenshot.name || `screenshot.png`
    storageKey = buildAttachmentStorageKey(issueId, attachmentId, filename)
    const body = new Uint8Array(await screenshot.arrayBuffer())
    dimensions = getImageDimensions(body)
    await uploadObject({
      body,
      contentLength: screenshot.size,
      contentType: screenshot.type,
      key: storageKey,
    })
  }

  const description = buildWidgetDescription({
    userText: fields.data.description,
    screenshotAttachmentId: attachmentId,
    widgetName: config.name,
    reporterName: fields.data.name ?? null,
    reporterEmail: fields.data.email ?? null,
    meta: {
      pageUrl: meta.data.url ?? null,
      userAgent: args.userAgent,
      viewportWidth: meta.data.viewportWidth ?? null,
      viewportHeight: meta.data.viewportHeight ?? null,
      screenWidth: meta.data.screenWidth ?? null,
      screenHeight: meta.data.screenHeight ?? null,
      devicePixelRatio: meta.data.devicePixelRatio ?? null,
    },
    customData,
  })

  try {
    // Direct insert with the attachment row in the SAME transaction: the
    // tRPC create's "no images at create time" rule exists because client
    // uploads happen after create — here the attachment exists before commit,
    // so the embedded image URL is valid the moment the issue is visible.
    // No ensureSubscribed / notification calls: the creator is an isAgent
    // user (skipped anyway); widget triage is pull-based via the project view.
    return await db.transaction(async (tx) => {
      await generateTxId(tx)
      const [issue] = await tx
        .insert(issues)
        .values({
          id: issueId,
          projectId: config.projectId,
          title: fields.data.title,
          status: `backlog`,
          priority: `none`,
          description,
          creatorId: config.widgetUserId,
        })
        .returning({ id: issues.id, identifier: issues.identifier })

      if (screenshot && attachmentId && storageKey) {
        await tx.insert(attachments).values({
          id: attachmentId,
          workspaceId: config.workspaceId,
          projectId: config.projectId,
          issueId,
          uploaderId: config.widgetUserId,
          filename: screenshot.name || `screenshot.png`,
          contentType: screenshot.type,
          sizeBytes: screenshot.size,
          storageKey,
          url: buildAttachmentUrl(attachmentId),
          width: dimensions?.width ?? null,
          height: dimensions?.height ?? null,
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
          workspaceId: config.workspaceId,
          projectId: config.projectId,
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
  } catch (error) {
    if (storageKey) {
      try {
        await deleteObject(storageKey)
      } catch (deleteError) {
        console.error(
          `Failed to rollback widget screenshot object`,
          deleteError
        )
      }
    }
    throw error
  }
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
  // A trashed target board reports disabled so embedded widgets hide instead
  // of erroring.
  if (!config.enabled || config.projectDeletedAt != null) {
    return jsonResponse(200, { enabled: false }, cors)
  }

  const form = config.formConfig ?? {}
  return jsonResponse(
    200,
    {
      enabled: true,
      form: {
        buttonLabel:
          typeof form.buttonLabel === `string` ? form.buttonLabel : null,
        accentColor:
          typeof form.accentColor === `string` ? form.accentColor : null,
        position:
          form.position === `bottom-left` ? `bottom-left` : `bottom-right`,
        emailRequired: form.emailRequired === true,
      },
      limits: { maxScreenshotBytes: maxImageUploadBytes },
    },
    { ...cors, "Cache-Control": `public, max-age=300` }
  )
}
