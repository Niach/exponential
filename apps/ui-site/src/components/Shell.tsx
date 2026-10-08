/* The site chrome: the header and footer bars shared with exponential.at
   (@exp/site-shell), bound to ui.exponential.at's nav. */
import type { ReactNode } from "react"
import { GitHubStarsButton, SiteFooterBar, SiteHeaderBar } from "@exp/site-shell"
import { NAV } from "../lib/nav"
import { LINKS } from "../lib/site"

export function Shell({ path, children }: { path: string; children: ReactNode }) {
  return (
    <>
      <SiteHeaderBar
        brand="Exponential UI"
        nav={[...NAV]}
        currentPath={path}
        right={
          <>
            <GitHubStarsButton variant="compact" />
            <a className="btn btn-ghost btn-sm" href={LINKS.marketing}>
              exponential.at
            </a>
            <a className="btn btn-primary btn-sm" href="/guides/react/">
              Get started
            </a>
          </>
        }
      />
      <main>{children}</main>
      <SiteFooterBar
        brand="Exponential UI"
        groups={[
          {
            links: [
              { label: `Components`, href: `/components/` },
              { label: `Themes`, href: `/themes/` },
              { label: `Guides`, href: `/guides/` },
              { label: `Playground`, href: `/playground/` },
              { label: `Conformance`, href: `/conformance/` },
            ],
          },
          {
            links: [
              { label: `Exponential`, href: LINKS.marketing },
              { label: `GitHub`, href: LINKS.repo },
              { label: `App styleguide`, href: LINKS.styleguide },
              { label: `Privacy`, href: `${LINKS.marketing}privacy/` },
              { label: `Imprint`, href: `${LINKS.marketing}imprint/` },
            ],
          },
        ]}
        legal={
          <>
            &copy; 2026 &middot;{` `}
            <a href={`${LINKS.repo}/blob/master/LICENSE`} style={{ color: `inherit` }}>
              Apache-2.0
            </a>
          </>
        }
      />
    </>
  )
}
