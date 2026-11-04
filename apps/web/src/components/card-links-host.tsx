import { useMemo, type ReactNode } from "react"
import { useNavigate } from "@tanstack/react-router"
import { CardLinksProvider, type CardLinks } from "@exp/ui"
import { useIssueRefs } from "@/components/issue-ref-provider"
import { appLinkPath, cardLinkOpens, classifyAppLink, isRoutedAppPath, issueTextLinks } from "@/lib/app-link"

/** #EXP-1674 (plan-card.json): a chat card's links go through THE classifier;
 *  `~/x` = the team on screen (`linkBase`); a Row subtitle's `#IDENT` refs that
 *  resolve in the team become links. */
export function CardLinksHost({ teamSlug, children }: { teamSlug: string; children: ReactNode }) {
  const navigate = useNavigate()
  const resolve = useIssueRefs()?.resolve
  const links = useMemo<CardLinks>(
    () => ({
      linkBase: `/t/${encodeURIComponent(teamSlug)}`,
      openUrl: (url) => {
        const origin = window.location.origin
        if (!cardLinkOpens(url, origin, teamSlug)) return
        const link = classifyAppLink(url, origin, teamSlug)
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
