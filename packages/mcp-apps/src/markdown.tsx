import { useEffect, useMemo, useState } from "react"
import { AppMarkdown, type AppMarkdownHost as HostPlugin } from "@exp/ui"
import { attachmentIdFromUrl, signedAttachmentUrl, useMcpActions } from "./actions"

// EXP-1183 — issue descriptions, comments and run reports are plain GFM
// (CLAUDE.md "Markdown"). #VAPP-258: the views paint them on genUI's
// Markdown (`AppMarkdown`, the app's own read-only renderer; raw HTML is
// never markup there). Two departures, both forced by the sandbox: an
// attachment picture is an authenticated `/api/attachments` URL, so it loads
// through the attachment's signed URL (`exponential_attachments_get` through
// the host; until then, and for any foreign picture, its alt text); links go
// through the host (`onOpenLink`), never a navigation.

/** The attachment ids `source`'s pictures name. */
export function attachmentImageIds(source: string): string[] {
  const ids = new Set<string>()
  for (const m of source.matchAll(/!\[[^\]]*\]\(([^)\s]+)/g)) {
    const id = attachmentIdFromUrl(m[1])
    if (id) ids.add(id)
  }
  return [...ids]
}

/** The view's host: http(s) links open through the host, relative ones
 *  against the app origin; a picture loads only from its signed URL. */
export function markdownHost(signed: Readonly<Record<string, string>>, origin?: string, onOpenLink?: (url: string) => void): HostPlugin {
  return {
    openUrl: (url) => {
      if (/^https?:/i.test(url)) onOpenLink?.(url)
    },
    urls: origin ? { baseUrl: origin } : undefined,
    mediaRequest: (src) => {
      const id = attachmentIdFromUrl(src)
      const url = id ? signed[id] : undefined
      return url ? { url, headers: {} } : null
    },
    markdownOptions: { density: `chat` },
  }
}

export function Markdown({
  source,
  origin,
  onOpenLink,
  className,
}: {
  source: string
  /** The app origin relative links resolve against. */
  origin?: string
  onOpenLink?: (url: string) => void
  className?: string
}) {
  const { call } = useMcpActions()
  const [signed, setSigned] = useState<Record<string, string>>({})
  const ids = attachmentImageIds(source).join(` `)
  // Attachment pictures load once their signed URL is back.
  useEffect(() => {
    let live = true
    for (const id of ids ? ids.split(` `) : []) {
      void signedAttachmentUrl(call, id).then((url) => {
        if (live && url) setSigned((prev) => (prev[id] === url ? prev : { ...prev, [id]: url }))
      })
    }
    return () => {
      live = false
    }
  }, [ids, call])
  const host = useMemo(() => markdownHost(signed, origin, onOpenLink), [signed, origin, onOpenLink])
  return <AppMarkdown text={source} host={host} className={className} />
}
