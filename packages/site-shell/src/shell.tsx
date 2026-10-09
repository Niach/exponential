/* The header and footer bars every public Exponential site shares
   (exponential.at, ui.exponential.at): same markup, same classes
   (styles/site.css, styles/site-footer.css), each site passes its own brand,
   nav, right-hand actions and footer groups. Site-specific behaviour (the
   marketing site's feedback widget and attribution forwarding) stays in the
   site's own wrapper. */
import { useEffect, useState, type ReactNode } from "react"
import { ExpLogo } from "./logo"

export type ShellLink = { label: string; href: string }

/** True once the page scrolled past 8 px (the topbar turns to glass). */
export function useScrolled(): boolean {
  const [scrolled, setScrolled] = useState(false)
  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 8)
    onScroll()
    window.addEventListener(`scroll`, onScroll, { passive: true })
    return () => window.removeEventListener(`scroll`, onScroll)
  }, [])
  return scrolled
}

export function SiteHeaderBar({
  brand,
  homeHref = `/`,
  nav,
  right,
  currentPath,
}: {
  brand: ReactNode
  homeHref?: string
  nav: ShellLink[]
  right?: ReactNode
  /** Marks the nav link whose href prefixes it as current. */
  currentPath?: string
}) {
  const scrolled = useScrolled()
  return (
    <header className={`topbar${scrolled ? ` is-scrolled` : ``}`}>
      <div className="shell topbar-inner">
        <a className="brand" href={homeHref}>
          <ExpLogo size={22} />
          <span>{brand}</span>
        </a>
        <nav className="nav">
          {nav.map((link) => {
            const current =
              currentPath !== undefined &&
              link.href !== `/` &&
              currentPath.startsWith(link.href)
            return (
              <a
                key={link.href}
                href={link.href}
                aria-current={current ? `page` : undefined}
              >
                {link.label}
              </a>
            )
          })}
        </nav>
        {right && <div className="topbar-right">{right}</div>}
      </div>
    </header>
  )
}

export function SiteFooterBar({
  brand,
  groups,
  legal,
}: {
  brand: ReactNode
  groups: { links: ShellLink[] }[]
  legal: ReactNode
}) {
  return (
    <footer>
      <div className="shell">
        <div className="foot-bottom">
          <span
            style={{ display: `inline-flex`, alignItems: `center`, gap: 8 }}
          >
            <ExpLogo size={16} />
            <span>{brand}</span>
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
            <span className="foot-legal">{legal}</span>
          </span>
        </div>
      </div>
    </footer>
  )
}
