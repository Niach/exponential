import type { WidgetConfigFailureCode, WidgetRuntimeState } from "./types"
import type { EnvMeta } from "./env-meta"
import { screenshotFilename } from "./capture/image"
import {
  formatWait,
  messagesFor,
  widgetMessages,
  type WidgetMessages,
} from "./i18n"

const messagesForEnglish = widgetMessages.en

export interface WidgetErrorBody {
  error?: string
  code?: string
  retryAfterSeconds?: number
}

// What an error response means for the widget (NEW-19). Current servers send
// a structured `code`; older ones only a status + frozen English text, so
// both are matched.
export type WidgetFailureKind =
  | WidgetConfigFailureCode
  | `rate_limited`
  | `too_large`
  | null

export function classifyFailure(
  status: number,
  body: WidgetErrorBody | null
): WidgetFailureKind {
  const code = body?.code
  if (
    code === `origin_not_allowed` ||
    code === `unknown_key` ||
    code === `widget_disabled` ||
    code === `rate_limited` ||
    code === `too_large`
  ) {
    return code
  }
  if (code) return null
  const error = body?.error ?? ``
  if (status === 429) return `rate_limited`
  if (status === 413) return `too_large`
  if (status === 403 && error === `Origin not allowed`) {
    return `origin_not_allowed`
  }
  if (
    status === 403 &&
    (error === `Widget is disabled` ||
      error === `This feedback board is unavailable`)
  ) {
    return `widget_disabled`
  }
  if (status === 404 && (error === `Unknown widget key` || !error)) {
    return `unknown_key`
  }
  return null
}

function pageOrigin(): string {
  try {
    return window.location.origin
  } catch {
    return `this page`
  }
}

// The integrator-facing explanation, logged to the console (the reporter
// only sees the localized message).
export function warnMisconfiguration(code: WidgetConfigFailureCode): void {
  const origin = pageOrigin()
  console.warn(
    code === `origin_not_allowed`
      ? `[ExponentialWidget] ${origin} is not allowed to use this widget key. Add ${origin} to the widget key's allowed domains in the widget settings in Exponential.`
      : code === `unknown_key`
        ? `[ExponentialWidget] unknown widget key: check the key passed to init() against the widget settings in Exponential.`
        : `[ExponentialWidget] this widget is disabled (or its board is unavailable); enable it in Exponential to show the launcher on ${origin}.`
  )
}

// Seconds to wait, from the Retry-After header (delta-seconds or an HTTP
// date) or the body's retryAfterSeconds; null = unknown.
export function retryAfterSeconds(
  headers: { get(name: string): string | null } | null | undefined,
  body: WidgetErrorBody | null
): number | null {
  const raw = headers?.get?.(`Retry-After`)?.trim()
  if (raw) {
    if (/^\d+(\.\d+)?$/.test(raw)) return Number(raw)
    const at = Date.parse(raw)
    if (!Number.isNaN(at)) return Math.max(0, (at - Date.now()) / 1000)
  }
  const fromBody = body?.retryAfterSeconds
  return typeof fromBody === `number` && Number.isFinite(fromBody) && fromBody >= 0
    ? fromBody
    : null
}

export function failureMessage(
  kind: WidgetFailureKind,
  body: WidgetErrorBody | null,
  wait: number | null,
  messages: WidgetMessages
): string {
  switch (kind) {
    case `origin_not_allowed`:
      return messages.originNotAllowed
    case `unknown_key`:
      return messages.unknownKey
    case `widget_disabled`:
      return messages.widgetDisabled
    case `rate_limited`:
      return messages.rateLimited(
        wait === null ? null : formatWait(wait, messages)
      )
    case `too_large`:
      return messages.requestTooLarge
    default:
      // The server's own (English) text is the most specific thing we have;
      // a non-English widget falls back to its generic line instead.
      return messages === messagesForEnglish && body?.error
        ? body.error
        : messages.genericError
  }
}

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
      // The server's own `error` text, untranslated (null on a network
      // error) — App matches old code-less servers' frozen copy on it.
      serverMessage?: string | null
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
  const messages = messagesFor(state)
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
    if (!response.ok) {
      const body = (await response
        .json()
        .catch(() => null)) as WidgetErrorBody | null
      const kind = classifyFailure(response.status, body)
      if (
        kind === `origin_not_allowed` ||
        kind === `unknown_key` ||
        kind === `widget_disabled`
      ) {
        warnMisconfiguration(kind)
      }
      return {
        ok: false,
        message: failureMessage(
          kind,
          body,
          kind === `rate_limited`
            ? retryAfterSeconds(response.headers, body)
            : null,
          messages
        ),
        status: response.status,
        serverMessage: typeof body?.error === `string` ? body.error : null,
        code:
          body?.code ??
          (kind === `rate_limited` || kind === null || kind === `too_large`
            ? null
            : kind),
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
      message: messages.networkError,
      status: null,
      code: null,
    }
  }
}
