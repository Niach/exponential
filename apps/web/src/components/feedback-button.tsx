import { useEffect, useState } from "react"
import { Megaphone } from "lucide-react"
import { useNavigate } from "@tanstack/react-router"
import {
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"
import { getRuntimeConfig } from "@/lib/runtime-config"

// One-shot fetch — runtime config is set at deploy time and won't change
// during a session. Cached at module scope so subsequent mounts don't refetch.
let cachedFeedbackUrl: string | null | undefined = undefined
let cachePromise: Promise<string | null> | null = null

async function loadFeedbackUrl(): Promise<string | null> {
  if (cachedFeedbackUrl !== undefined) return cachedFeedbackUrl
  if (cachePromise) return cachePromise
  cachePromise = getRuntimeConfig()
    .then((config) => {
      cachedFeedbackUrl = config.publicFeedbackUrl
      return cachedFeedbackUrl
    })
    .catch(() => {
      cachedFeedbackUrl = null
      return null
    })
  return cachePromise
}

export function FeedbackButton() {
  const navigate = useNavigate()
  const [externalUrl, setExternalUrl] = useState<string | null>(
    cachedFeedbackUrl ?? null
  )

  useEffect(() => {
    if (cachedFeedbackUrl !== undefined) return
    void loadFeedbackUrl().then(setExternalUrl)
  }, [])

  const handleClick = () => {
    if (externalUrl) {
      const source =
        typeof window !== `undefined` ? window.location.hostname : ``
      const url = new URL(`${externalUrl}/feedback`)
      if (source) url.searchParams.set(`source`, source)
      window.open(url.toString(), `_blank`, `noopener,noreferrer`)
      return
    }
    void navigate({ to: `/feedback`, search: {} })
  }

  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        onClick={handleClick}
        aria-label="Send feedback"
        className="text-muted-foreground"
      >
        <Megaphone className="size-4" />
        <span>Send feedback</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  )
}
