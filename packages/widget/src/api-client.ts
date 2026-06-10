import type { WidgetRuntimeState } from "./types"
import type { EnvMeta } from "./env-meta"
import { screenshotFilename } from "./capture/image"

export type SubmitResult =
  | { ok: true; identifier: string | null }
  | { ok: false; message: string }

export async function submitFeedback(args: {
  state: WidgetRuntimeState
  title: string
  description: string
  email: string | null
  screenshot: Blob | null
  meta: EnvMeta
}): Promise<SubmitResult> {
  const { state } = args
  const formData = new FormData()
  formData.set(`key`, state.options.key)
  formData.set(`title`, args.title)
  formData.set(`description`, args.description)
  if (args.email) formData.set(`email`, args.email)
  if (state.identity.name) formData.set(`name`, state.identity.name)
  if (state.identity.userId) formData.set(`userId`, state.identity.userId)
  if (Object.keys(state.customData).length > 0) {
    formData.set(`customData`, JSON.stringify(state.customData))
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
      return { ok: false, message: `Too many reports right now — try again in a minute.` }
    }
    if (!response.ok) {
      const body = (await response.json().catch(() => null)) as {
        error?: string
      } | null
      return {
        ok: false,
        message: body?.error ?? `Something went wrong. Please try again.`,
      }
    }
    const body = (await response.json().catch(() => null)) as {
      identifier?: string
    } | null
    return { ok: true, identifier: body?.identifier ?? null }
  } catch {
    return { ok: false, message: `Network error. Please try again.` }
  }
}
