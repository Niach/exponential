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
          <a href="/#install">Self-host</a>
          <a href="/#open-source">Open source</a>
        </nav>
        <div className="topbar-right">
          <a
            className="btn btn-ghost btn-sm"
            href="https://github.com/Niach/exponential"
          >
            <IcGithub size={14} /> GitHub
          </a>
          <a className="btn btn-primary btn-sm" href="/#install">
            Self-host <IcArrow size={12} />
          </a>
        </div>
      </div>
    </header>
  )
}

export function SiteFooter() {
  return (
    <footer>
      <div className="shell">
        <div className="foot-bottom">
          <span
            style={{ display: "inline-flex", alignItems: "center", gap: 8 }}
          >
            <ExpLogo size={16} />
            <span>Exponential</span>
          </span>
          <span style={{ display: "inline-flex", gap: 16 }}>
            <a href="/privacy/" style={{ color: "inherit" }}>
              Privacy
            </a>
            <a href="/terms/" style={{ color: "inherit" }}>
              Terms
            </a>
            <span>© 2026 — released under MIT</span>
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
