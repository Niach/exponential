import { useEffect, type ReactNode } from "react"
import { GitHubStarsButton, SiteFooterBar, SiteHeaderBar } from "@exp/site-shell"
import { initAttributionForwarding } from "../lib/attribution"
import { LINKS } from "../lib/links"
import { DownloadIconRow } from "./DownloadSection"
import { IcArrow } from "./icons"
import { WidgetEmbed } from "./WidgetEmbed"

export function SiteHeader() {
  useEffect(() => {
    /* Cookieless ref/utm forwarding onto app + internal links (EXP-362);
       every page renders SiteHeader once, and the module self-guards. */
    initAttributionForwarding()
  }, [])

  return (
    <>
      {/* Every page renders SiteHeader exactly once, so this puts the
          feedback widget on all routes (WidgetEmbed renders nothing and
          guards against double-injection). */}
      <WidgetEmbed />
      {/* The bar itself is shared with ui.exponential.at (@exp/site-shell). */}
      <SiteHeaderBar
        brand="Exponential"
        nav={[
          { label: `Product`, href: `/#product` },
          { label: `Pricing`, href: `/pricing/` },
          { label: `Docs`, href: `/docs/` },
          { label: `UI SDK`, href: LINKS.ui },
          { label: `Download`, href: LINKS.downloadPage },
        ]}
        right={
          <>
            <GitHubStarsButton variant="compact" />
            <a className="btn btn-sm topbar-dl" href={LINKS.downloadPage}>
              Download
            </a>
            <a className="btn btn-ghost btn-sm" href={LINKS.app.login}>
              Sign in
            </a>
            <a className="btn btn-primary btn-sm" href={LINKS.app.login}>
              Get started free
            </a>
          </>
        }
      />
    </>
  )
}

export function FooterCTA({
  title = `Bring your team to the next level`,
  subtitle = `The only tool your team will need`,
}: {
  title?: string
  subtitle?: string
}) {
  return (
    <section className="footer-cta">
      <div className="shell footer-cta-inner">
        <h2>{title}</h2>
        <p>{subtitle}</p>
        {/* EXP-176: the Self-host button moved into the home pricing
            section (next to "Compare all plans"); /pricing has its own
            self-host section and SiteFooter links it site-wide. */}
        <div className="footer-cta-buttons">
          <a className="btn btn-primary" href={LINKS.app.login}>
            Sign up free <IcArrow size={12} />
          </a>
        </div>
        <div className="footer-cta-dl">
          <DownloadIconRow />
        </div>
      </div>
    </section>
  )
}

export function SiteFooter() {
  return (
    <SiteFooterBar
      brand="Exponential"
      groups={[
        {
          links: [
            { label: `Pricing`, href: `/pricing/` },
            { label: `Download`, href: LINKS.downloadPage },
            { label: `Docs`, href: `/docs/` },
            { label: `Self-host`, href: `/docs/self-host/` },
            { label: `UI SDK`, href: LINKS.ui },
          ],
        },
        {
          links: [
            { label: `GitHub`, href: LINKS.github.repo },
            { label: `Contact`, href: `/contact/` },
            { label: `Privacy`, href: `/privacy/` },
            { label: `Terms`, href: `/terms/` },
            { label: `Imprint`, href: `/imprint/` },
          ],
        },
      ]}
      legal={
        <>
          &copy; 2026 &middot;{` `}
          <a
            href={`${LINKS.github.repo}/blob/master/LICENSE`}
            style={{ color: `inherit` }}
          >
            Apache-2.0 (open source)
          </a>
        </>
      }
    />
  )
}

export function SiteShell({ children }: { children: ReactNode }) {
  return (
    <>
      <SiteHeader />
      <main>{children}</main>
      <SiteFooter />
    </>
  )
}
