import { useEffect, useState } from "react"
import { useSession } from "@/hooks/use-session"
import {
  getRuntimeConfig,
  type FeedbackWidgetConfig,
} from "@/lib/runtime-config"
import type { ExponentialWidgetStub, QueuedCall } from "@exp/widget/types"

// In-app mount of the embeddable feedback widget: the same loader script a
// customer would paste, pointed at the cloud's own feedback config (key
// hardcoded in lib/runtime-config.ts). Cloud-only (runtime config carries no
// widget on self-hosted instances — there the sidebar FeedbackButton renders
// nothing). Installed once per page load.
type LoadStatus = `idle` | `loading` | `ready` | `failed`
let status: LoadStatus = `idle`

// Same stub the public paste-in snippet creates — the in-app path exercises
// the real loader handshake instead of a privileged shortcut.
function installSnippetStub(): void {
  if (window.ExponentialWidget) return
  const queue: QueuedCall[] = []
  const api = { q: queue } as unknown as ExponentialWidgetStub
  for (const method of [
    `init`,
    `identify`,
    `setCustomData`,
    `open`,
    `close`,
    `submit`,
  ] as const) {
    // Like the public snippet, queued stub calls are fire-and-forget — a
    // pre-loader submit() returns undefined, not the loader's Promise.
    api[method] = ((...args: unknown[]) => {
      queue.push([method, args])
    }) as never
  }
  window.ExponentialWidget = api
}

// Returns true when the click was handled (widget open queued or shown);
// false tells the caller to use its legacy fallback path.
export function openFeedbackWidget(): boolean {
  if (status === `idle` || status === `failed`) return false
  window.ExponentialWidget?.open()
  return true
}

export function FeedbackWidgetProvider() {
  const { data: session } = useSession()
  const [widget, setWidget] = useState<FeedbackWidgetConfig | null>(null)

  useEffect(() => {
    getRuntimeConfig()
      .then((config) => setWidget(config.feedbackWidget))
      .catch(() => setWidget(null))
  }, [])

  useEffect(() => {
    if (!widget || status !== `idle`) return
    status = `loading`

    const scriptUrl = widget.scriptUrl.startsWith(`http`)
      ? widget.scriptUrl
      : `${window.location.origin}${widget.scriptUrl}`

    installSnippetStub()
    window.ExponentialWidget!.init({
      key: widget.widgetKey,
      // Dogfood the floating launcher like any customer site (EXP-163). The
      // sidebar's FeedbackButton stays as a second entry point. Desktop is
      // pinned to the bottom-right fab regardless of the remote config;
      // mobile is deliberately left to the config/default (edge tab, middle
      // right — EXP-569), which is what replaced the EXP-449 tab-bar-
      // collision CSS hack.
      launcher: { desktop: { mode: `fab`, position: `bottom-right` } },
      // Cache-skew bridge: a cached pre-EXP-569 loader ignores `launcher`
      // and still honors this; the new loader ignores `position` whenever
      // `launcher` is present.
      position: `bottom-right`,
    })
    window.ExponentialWidget!.setCustomData({
      app: `exponential-web`,
      instance: window.location.hostname,
    })

    const script = document.createElement(`script`)
    script.async = true
    script.src = scriptUrl
    script.onload = () => {
      status = `ready`
    }
    script.onerror = () => {
      status = `failed`
    }
    document.head.appendChild(script)
  }, [widget])

  useEffect(() => {
    if (!widget || status === `idle` || status === `failed`) return
    const user = session?.user
    if (!user) return
    window.ExponentialWidget?.identify({
      email: user.email ?? undefined,
      name: user.name ?? undefined,
      userId: user.id,
    })
  }, [widget, session])

  return null
}
