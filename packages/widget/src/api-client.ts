import type { WidgetRuntimeState } from "./types"
import type { EnvMeta } from "./env-meta"
import { screenshotFilename } from "./capture/image"

export type SubmitResult =
  | {
      ok: true
      identifier: string | null
      url: string | null
      // Support mode only (REV2-10): did the server actually SEND the
      // confirmation email carrying the reporter's magic link — their only
      // credential for the conversation? `null` when it doesn't apply
      // (feedback submissions) or when an older server doesn't report it, in
      // which case the panel keeps its optimistic "check your email" copy.
      // Optional so the field stays additive for callers that construct a
      // result themselves.
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

// Support mode (EXP-130): files a helpdesk ticket. The reply channel is the
// reporter's email — the server mails them a magic conversation link.
export async function submitSupportRequest(args: {
  state: WidgetRuntimeState
  message: string
  email: string
  // Overrides for the identify()-time values (a panel-typed name) and the
  // setCustomData blob (caller pre-merges field values); absent = legacy
  // state fallbacks.
  name?: string | null
  customData?: Record<string, string | number | boolean>
  // The Panel honeypot's value — only a bot ever fills it, and the server
  // drops those submissions with a fake success (REV2-69).
  website?: string
  meta: EnvMeta
}): Promise<SubmitResult> {
  const { state } = args
  const formData = new FormData()
  formData.set(`key`, state.options.key)
  formData.set(`mode`, `support`)
  formData.set(`message`, args.message)
  formData.set(`email`, args.email)
  if (args.website) formData.set(`website`, args.website)
  const name = args.name ?? state.identity.name
  if (name) formData.set(`name`, name)
  if (state.identity.userId) formData.set(`userId`, state.identity.userId)
  const customData = args.customData ?? state.customData
  if (Object.keys(customData).length > 0) {
    formData.set(`customData`, JSON.stringify(customData))
  }
  formData.set(`meta`, JSON.stringify(args.meta))

  try {
    const response = await fetch(`${state.apiOrigin}/api/widget/submit`, {
      method: `POST`,
      body: formData,
      credentials: `omit`,
    })
    if (response.status === 429) {
      return {
        ok: false,
        message: `Too many requests right now — try again in a minute.`,
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
      emailDelivered?: boolean | null
    } | null
    return {
      ok: true,
      identifier: body?.identifier ?? null,
      url: null,
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

export async function submitFeedback(args: {
  state: WidgetRuntimeState
  title: string
  description: string
  email: string | null
  // Overrides for the identify()-time values (a panel-typed name) and the
  // setCustomData blob (caller pre-merges custom-field values); absent =
  // legacy state fallbacks.
  name?: string | null
  customData?: Record<string, string | number | boolean>
  screenshot: Blob | null
  // See submitSupportRequest — the honeypot rides both forms.
  website?: string
  meta: EnvMeta
}): Promise<SubmitResult> {
  const { state } = args
  const formData = new FormData()
  formData.set(`key`, state.options.key)
  formData.set(`title`, args.title)
  formData.set(`description`, args.description)
  if (args.email) formData.set(`email`, args.email)
  if (args.website) formData.set(`website`, args.website)
  const name = args.name ?? state.identity.name
  if (name) formData.set(`name`, name)
  if (state.identity.userId) formData.set(`userId`, state.identity.userId)
  const customData = args.customData ?? state.customData
  if (Object.keys(customData).length > 0) {
    formData.set(`customData`, JSON.stringify(customData))
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

  try {
    const response = await fetch(`${state.apiOrigin}/api/widget/submit`, {
      method: `POST`,
      body: formData,
      credentials: `omit`,
    })
    if (response.status === 429) {
      return {
        ok: false,
        message: `Too many reports right now — try again in a minute.`,
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
    } | null
    return {
      ok: true,
      identifier: body?.identifier ?? null,
      url: typeof body?.url === `string` ? body.url : null,
      // Feedback submissions send the reporter no email at all.
      emailDelivered: null,
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
