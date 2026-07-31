import { useEffect, useState, type ReactNode } from "react"
import { initAttributionForwarding } from "../lib/attribution"
import { LINKS } from "../lib/links"
import { DownloadIconRow } from "./DownloadSection"
import { GitHubStarsButton } from "./GitHubStarsButton"
import { ExpLogo, IcArrow } from "./icons"
import { WidgetEmbed } from "./WidgetEmbed"

export function SiteHeader() {
  /* Transparent at rest, glass once scrolled (site.css .is-scrolled). */
  const [scrolled, setScrolled] = useState(false)
  useEffect(() => {
    /* Cookieless ref/utm forwarding onto app + internal links (EXP-362);
       every page renders SiteHeader once, and the module self-guards. */
    initAttributionForwarding()
    const onScroll = () => setScrolled(window.scrollY > 8)
    onScroll()
    window.addEventListener(`scroll`, onScroll, { passive: true })
    return () => window.removeEventListener(`scroll`, onScroll)
  }, [])

  return (
    <>
      {/* Every page renders SiteHeader exactly once, so this puts the
          feedback widget on all routes (WidgetEmbed renders nothing and
          guards against double-injection). */}
      <WidgetEmbed />
      <header className={`topbar${scrolled ? ` is-scrolled` : ``}`}>
        <div className="shell topbar-inner">
          <a className="brand" href="/">
            <ExpLogo size={22} />
            <span>Exponential</span>
          </a>
          <nav className="nav">
            <a href="/#product">Product</a>
            <a href="/pricing/">Pricing</a>
            <a href="/docs/">Docs</a>
            <a href={LINKS.downloadPage}>Download</a>
          </nav>
          <div className="topbar-right">
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
          </div>
        </div>
      </header>
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
  const groups = [
    {
      links: [
        { label: `Pricing`, href: `/pricing/` },
        { label: `Download`, href: LINKS.downloadPage },
        { label: `Docs`, href: `/docs/` },
        { label: `Self-host`, href: `/docs/self-host/` },
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
  ]

  return (
    <footer>
      <div className="shell">
        <div className="foot-bottom">
          <span
            style={{ display: `inline-flex`, alignItems: `center`, gap: 8 }}
          >
            <ExpLogo size={16} />
            <span>Exponential</span>
          </span>
          <span className="foot-groups">
            {groups.map((g) => (
              <span key={g.links[0].label} className="foot-group">
                {g.links.map((l) => (
                  <a key={l.label} href={l.href} style={{ color: `inherit` }}>
                    {l.label}
                  </a>
                ))}
              </span>
            ))}
            <span className="foot-legal">
              &copy; 2026 &middot;{` `}
              <a
                href={`${LINKS.github.repo}/blob/master/LICENSE`}
                style={{ color: `inherit` }}
              >
                Apache-2.0 — open source
              </a>
            </span>
          </span>
        </div>
      </div>
    </footer>
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
