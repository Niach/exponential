import type { ReactNode } from "react"
import { ExpLogo, IcArrow, IcGithub } from "./icons"

export function SiteHeader() {
  return (
    <header className="topbar">
      <div className="shell topbar-inner">
        <a className="brand" href="/">
          <ExpLogo size={22} />
          <span>Exponential</span>
        </a>
        <nav className="nav">
          <a href="/#features">Features</a>
          <a href="/#mobile">Mobile</a>
          <a href="/#agents">Agents</a>
          <a href="/docs/">Docs</a>
        </nav>
        <div className="topbar-right">
          <a
            className="btn btn-ghost btn-sm"
            href="https://github.com/Niach/exponential"
          >
            <IcGithub size={14} /> GitHub
          </a>
          <a
            className="btn btn-ghost btn-sm"
            href="https://app.exponential.at/auth/login"
          >
            Sign in
          </a>
          <a
            className="btn btn-primary btn-sm"
            href="https://app.exponential.at/auth/register"
          >
            Get started free
          </a>
        </div>
      </div>
    </header>
  )
}

export function FooterCTA() {
  return (
    <section className="footer-cta">
      <div className="shell footer-cta-inner">
        <h2>Get Exponential.</h2>
        <p>Free for individuals and teams. No credit card required.</p>
        <div className="footer-cta-buttons">
          <a
            className="btn btn-primary"
            href="https://app.exponential.at/auth/register"
          >
            Sign up free <IcArrow size={12} />
          </a>
          <a className="btn btn-ghost" href="/docs/self-host/">
            Self-host
          </a>
        </div>
      </div>
    </section>
  )
}

export function SiteFooter() {
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
          <span style={{ display: `inline-flex`, gap: 16, flexWrap: `wrap` }}>
            <a
              href="https://app.exponential.at/feedback"
              style={{ color: `inherit` }}
            >
              Send feedback
            </a>
            <a href="/privacy/" style={{ color: `inherit` }}>
              Privacy
            </a>
            <a href="/terms/" style={{ color: `inherit` }}>
              Terms
            </a>
            <span>&copy; 2026 &mdash; Elastic License 2.0</span>
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
