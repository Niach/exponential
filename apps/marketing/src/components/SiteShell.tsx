import type { ReactNode } from "react"
import { LINKS } from "../lib/links"
import { DownloadIconRow } from "./DownloadSection"
import { ExpLogo, IcArrow, IcGithub } from "./icons"
import { WidgetEmbed } from "./WidgetEmbed"

export function SiteHeader() {
  return (
    <>
      {/* Every page renders SiteHeader exactly once, so this puts the
          feedback widget on all routes (WidgetEmbed renders nothing and
          guards against double-injection). */}
      <WidgetEmbed />
      <header className="topbar">
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
            <a className="btn btn-ghost btn-sm" href={LINKS.github.repo}>
              <IcGithub size={14} /> GitHub
            </a>
            <a className="btn btn-ghost btn-sm" href={LINKS.app.login}>
              Sign in
            </a>
            <a className="btn btn-primary btn-sm" href={LINKS.app.register}>
              Get started free
            </a>
          </div>
        </div>
      </header>
    </>
  )
}

export function FooterCTA({
  title = `Go Exponential.`,
  subtitle = `Free for individuals and teams. No credit card required.`,
}: {
  title?: string
  subtitle?: string
}) {
  return (
    <section className="footer-cta">
      <div className="shell footer-cta-inner">
        <h2>{title}</h2>
        <p>{subtitle}</p>
        <div className="footer-cta-buttons">
          <a className="btn btn-primary" href={LINKS.app.register}>
            Sign up free <IcArrow size={12} />
          </a>
          <a className="btn btn-ghost" href="/docs/self-host/">
            Self-host
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
      title: `Product`,
      links: [
        { label: `Pricing`, href: `/pricing/` },
        { label: `Download`, href: LINKS.downloadPage },
        { label: `Docs`, href: `/docs/` },
        { label: `Self-host`, href: `/docs/self-host/` },
      ],
    },
    {
      title: `Meta`,
      links: [
        { label: `GitHub`, href: LINKS.github.repo },
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
              <span key={g.title} className="foot-group">
                <span className="foot-group-title">{g.title}</span>
                {g.links.map((l) => (
                  <a key={l.label} href={l.href} style={{ color: `inherit` }}>
                    {l.label}
                  </a>
                ))}
              </span>
            ))}
            <span className="foot-legal">
              &copy; 2026 &mdash; Elastic License 2.0
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
      {children}
      <SiteFooter />
    </>
  )
}
