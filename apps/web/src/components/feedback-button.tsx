import { useEffect, useState } from "react"
import {
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"
import { conceptIcon } from "@/lib/icons.generated"
import { getRuntimeConfig } from "@/lib/runtime-config"
import { openFeedbackWidget } from "@/components/feedback-widget-provider"

// EXP-317: a cross-client CONCEPT, not a raw lucide import — the natives'
// bug-report entry uses the same glyph.
const ReportBugIcon = conceptIcon(`nav-report-bug`)

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

// Whether the embedded feedback widget is configured on this instance —
// shared by the sidebar entry and the mobile user menu (EXP-189).
export function useFeedbackWidgetAvailable(): boolean {
  const [available, setAvailable] = useState<boolean>(
    cachedWidgetAvailable ?? false
  )

  useEffect(() => {
    if (cachedWidgetAvailable !== undefined) return
    void loadWidgetAvailable().then(setAvailable)
  }, [])

  return available
}

// EXP-771: THE way into the embedded feedback widget on the web. The provider
// mounts it headless now (no floating launcher anywhere), so this muted
// sidebar-footer entry — sitting right above "Getting started", same shape,
// same muted weight — is the one that opens the panel. Renders nothing when
// the runtime config exposes no widget (every self-hosted instance); EXP-180
// removed the legacy public-feedback-board redirect fallback.
export function FeedbackButton() {
  const available = useFeedbackWidgetAvailable()

  if (!available) return null

  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        onClick={() => openFeedbackWidget()}
        aria-label="Report bug"
        className="text-muted-foreground"
      >
        <ReportBugIcon className="size-4" />
        <span>Report bug</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  )
}
