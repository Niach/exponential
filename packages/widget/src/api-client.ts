import type { WidgetRuntimeState } from "./types"
import type { EnvMeta } from "./env-meta"
import { screenshotFilename } from "./capture/image"

export type SubmitResult =
  | {
      ok: true
      identifier: string | null
      url: string | null
      // SLOP-4 / REV2-10: did the server actually SEND the confirmation
      // email carrying the reporter's magic link — their only credential for
      // the conversation? `null` when the reporter left no email (nothing to
      // send) or when an older server doesn't report it. Optional so the
      // field stays additive for callers that construct a result themselves.
      emailDelivered?: boolean | null
    }
  | {
      ok: false
      message: string
      // The HTTP status (null on a network error) and the server's additive
      // structured error code (null when absent) drive App's email-recovery
      // reveal.
      status: number | null
      code: string | null
    }

// SLOP-4: the ONE submit — every submission becomes an issue on the
// widget's board; `identifier` names it.
export async function submitFeedback(args: {
  state: WidgetRuntimeState
  // What the reporter wrote. The server titles the issue off its first line
  // unless a headless host passes an explicit `title`.
  message: string
  title?: string | null
  email: string | null
  // Overrides for the identify()-time values (a panel-typed name) and the
  // setCustomData blob (caller pre-merges custom-field values); absent =
  // legacy state fallbacks.
  name?: string | null
  customData?: Record<string, string | number | boolean>
  screenshot: Blob | null
  // Reporter-attached pictures (FEED-5), sent after the screenshot.
  images?: { blob: Blob; filename: string }[]
  // Reporter-picked label ids (EXP-435); the server drops anything outside
  // the widget's configured set.
  labelIds?: string[]
  // The Panel honeypot's value — only a bot ever fills it, and the server
  // drops those submissions with a fake success (REV2-69).
  website?: string
  meta: EnvMeta
}): Promise<SubmitResult> {
  const { state } = args
  const formData = new FormData()
  formData.set(`key`, state.options.key)
  formData.set(`message`, args.message)
  if (args.title) formData.set(`title`, args.title)
  if (args.email) formData.set(`email`, args.email)
  if (args.website) formData.set(`website`, args.website)
  const name = args.name ?? state.identity.name
  if (name) formData.set(`name`, name)
  if (state.identity.userId) formData.set(`userId`, state.identity.userId)
  const customData = args.customData ?? state.customData
  if (Object.keys(customData).length > 0) {
    formData.set(`customData`, JSON.stringify(customData))
  }
  if (args.labelIds && args.labelIds.length > 0) {
    formData.set(`labels`, JSON.stringify(args.labelIds))
  }
  formData.set(`meta`, JSON.stringify(args.meta))
  if (args.screenshot) {
    formData.set(
      `screenshot`,
      new File([args.screenshot], screenshotFilename(args.screenshot), {
        type: args.screenshot.type,
      })
    )
  }
  for (const image of args.images ?? []) {
    formData.append(
      `images`,
      new File([image.blob], image.filename, { type: image.blob.type })
    )
  }

  try {
    const response = await fetch(`${state.apiOrigin}/api/widget/submit`, {
      method: `POST`,
      body: formData,
      credentials: `omit`,
    })
    if (response.status === 429) {
      return {
        ok: false,
        message: `Too many reports right now. Try again in a minute.`,
        status: 429,
        code: null,
      }
    }
    if (!response.ok) {
      const body = (await response.json().catch(() => null)) as {
        error?: string
        code?: string
      } | null
      return {
        ok: false,
        message: body?.error ?? `Something went wrong. Please try again.`,
        status: response.status,
        code: body?.code ?? null,
      }
    }
    const body = (await response.json().catch(() => null)) as {
      identifier?: string
      // Absolute issue URL. Current servers always send null (EXP-180
      // removed public boards); tolerated for older self-hosted servers.
      url?: string | null
      emailDelivered?: boolean | null
    } | null
    return {
      ok: true,
      identifier: body?.identifier ?? null,
      url: typeof body?.url === `string` ? body.url : null,
      emailDelivered:
        typeof body?.emailDelivered === `boolean` ? body.emailDelivered : null,
    }
  } catch {
    return {
      ok: false,
      message: `Network error. Please try again.`,
      status: null,
      code: null,
    }
  }
}
