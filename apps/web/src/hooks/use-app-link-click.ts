import { useCallback, type MouseEvent } from "react"
import { useNavigate } from "@tanstack/react-router"
import { appLinkPath, classifyAppLink, isRoutedAppPath, teamSlugOfPath } from "@/lib/app-link"

/** EXP-1188: the click handler for a container of agent prose. A link into
 *  this instance (an issue, a run, any app page) routes in-app instead of
 *  reloading or opening a tab; a source opens in a new tab; a link with no
 *  real host opens nothing. Capture phase: the read-only editor never sees
 *  it. A modified click on an in-app link keeps the browser's own behaviour.
 *  A same-origin link the router does not serve (an attachment, a static
 *  file, the widget) is an ordinary link: the browser follows it as the
 *  anchor says (same tab, or its own `target`), never the SPA not-found. */
export function useAppLinkClick() {
  const navigate = useNavigate()
  return useCallback(
    (event: MouseEvent<HTMLElement>) => {
      const anchor = (event.target as Element | null)?.closest?.(`a[href]`)
      if (!anchor || !event.currentTarget.contains(anchor)) return
      const link = classifyAppLink(
        anchor.getAttribute(`href`) ?? ``,
        window.location.origin,
        // #EXP-1674: `~/x` resolves against the team on screen.
        teamSlugOfPath(window.location.pathname)
      )
      const modified =
        event.button !== 0 ||
        event.metaKey ||
        event.ctrlKey ||
        event.shiftKey ||
        event.altKey
      if (link.kind === `ignore`) {
        event.preventDefault()
        return
      }
      if (modified) return
      if (link.kind === `app` && !isRoutedAppPath(link.path)) return
      event.preventDefault()
      if (link.kind === `external`) {
        window.open(link.url, `_blank`, `noopener,noreferrer`)
        return
      }
      const path = appLinkPath(link)
      if (path) void navigate({ href: path })
    },
    [navigate]
  )
}
