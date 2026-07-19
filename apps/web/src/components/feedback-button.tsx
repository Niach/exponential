import { useEffect, useState } from "react"
import { LifeBuoy } from "lucide-react"
import {
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"
import { getRuntimeConfig } from "@/lib/runtime-config"
import { openFeedbackWidget } from "@/components/feedback-widget-provider"

// One-shot fetch — runtime config is set at deploy time and won't change
// during a session. Cached at module scope so subsequent mounts don't refetch.
let cachedWidgetAvailable: boolean | undefined = undefined
let cachePromise: Promise<boolean> | null = null

async function loadWidgetAvailable(): Promise<boolean> {
  if (cachedWidgetAvailable !== undefined) return cachedWidgetAvailable
  if (cachePromise) return cachePromise
  cachePromise = getRuntimeConfig()
    .then((config) => {
      cachedWidgetAvailable = config.feedbackWidget !== null
      return cachedWidgetAvailable
    })
    .catch(() => {
      cachedWidgetAvailable = false
      return false
    })
  return cachePromise
}

// Sidebar entry point into the embedded feedback widget (the same widget the
// FeedbackWidgetProvider mounts as a floating launcher). Renders nothing when
// the runtime config exposes no dogfood widget — EXP-180 removed the legacy
// public-feedback-board redirect fallback.
export function FeedbackButton() {
  const [available, setAvailable] = useState<boolean>(
    cachedWidgetAvailable ?? false
  )

  useEffect(() => {
    if (cachedWidgetAvailable !== undefined) return
    void loadWidgetAvailable().then(setAvailable)
  }, [])

  if (!available) return null

  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        onClick={() => openFeedbackWidget()}
        aria-label="Feedback & support"
        className="text-muted-foreground"
      >
        <LifeBuoy className="size-4" />
        <span>Feedback &amp; support</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  )
}
