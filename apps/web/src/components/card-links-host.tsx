import { useMemo, type ReactNode } from "react"
import { useNavigate } from "@tanstack/react-router"
import { CardLinksProvider, type CardLinks } from "@exp/ui"
import { useIssueRefs } from "@/components/issue-ref-provider"
import { appLinkPath, cardLinkOpens, classifyAppLink, isRoutedAppPath, issueTextLinks, teamShorthandOf } from "@/lib/app-link"

/** #EXP-1674 (plan-card.json): a chat card's links go through THE classifier
 *  with the team on screen as the `~/` team; a Row subtitle's `#IDENT` refs that
 *  resolve in the team become links. */
export function CardLinksHost({ teamSlug, children }: { teamSlug: string; children: ReactNode }) {
  const navigate = useNavigate()
  const resolve = useIssueRefs()?.resolve
  const links = useMemo<CardLinks>(
    () => ({
      openUrl: (url) => {
        const origin = window.location.origin
        const href = teamShorthandOf(url, origin)
        if (!cardLinkOpens(href, origin, teamSlug)) return
        const link = classifyAppLink(href, origin, teamSlug)
        const path = appLinkPath(link)
        if (path && (link.kind !== `app` || isRoutedAppPath(link.path))) void navigate({ href: path })
        else window.open(link.kind === `external` ? link.url : url, `_blank`, `noopener,noreferrer`)
      },
      textLinks: (text) => issueTextLinks(text, teamSlug, (identifier) => Boolean(resolve?.(identifier))),
    }),
    [navigate, resolve, teamSlug]
  )
  return <CardLinksProvider value={links}>{children}</CardLinksProvider>
}
