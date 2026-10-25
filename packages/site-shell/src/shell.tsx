/* The header and footer bars of exponential.at (apps/marketing, the /ui/
   section included): same markup, same classes (styles/site.css,
   styles/site-footer.css). Site behaviour (the feedback widget, attribution
   forwarding) stays in the site's own wrapper. */
import { useEffect, useRef, useState, type ReactNode } from "react"
import { ChevronLeft, Menu } from "lucide-react"
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

/** A level of the header below the main menu (exponential.at/ui/, /talk/):
 *  its own links, entered from the main link to `href`, left by the back
 *  button. */
export type ShellSection = { label: string; href: string; links: ShellLink[] }

const isCurrent = (link: ShellLink, currentPath?: string) =>
  currentPath !== undefined && link.href !== `/` && currentPath.startsWith(link.href)

/** The nav on phones (the top bar's links hide at <= 720px): the same level
 *  as the bar. Works without JS; with it, Escape or a tap outside closes it. */
function PhoneMenu({ children }: { children: ReactNode }) {
  const ref = useRef<HTMLDetailsElement>(null)
  useEffect(() => {
    const close = (focus: boolean) => {
      const el = ref.current
      if (!el?.open) return
      el.open = false
      if (focus) el.querySelector(`summary`)?.focus()
    }
    const onKey = (e: KeyboardEvent) => e.key === `Escape` && close(ref.current?.contains(document.activeElement) ?? false)
    const onDown = (e: PointerEvent) => !ref.current?.contains(e.target as Node) && close(false)
    document.addEventListener(`keydown`, onKey)
    document.addEventListener(`pointerdown`, onDown)
    return () => {
      document.removeEventListener(`keydown`, onKey)
      document.removeEventListener(`pointerdown`, onDown)
    }
  }, [])
  return (
    <details ref={ref} className="site-menu">
      <summary className="btn btn-ghost btn-sm site-icon-btn" aria-label="Menu">
        <Menu size={15} strokeWidth={1.8} aria-hidden />
      </summary>
      <nav aria-label="Site">{children}</nav>
    </details>
  )
}

/** ONE header for every page of exponential.at. A page inside a section
 *  (its path starts with the section's href) opens on that section's links
 *  with a back button in front; back steps up to the main menu in place (a
 *  link to `/` without JS), and a section's main link steps down into it. */
export function SiteHeaderBar({
  brand,
  homeHref = `/`,
  nav,
  sections = [],
  right,
  currentPath,
}: {
  brand: ReactNode
  homeHref?: string
  nav: ShellLink[]
  /** The sub-menus; a main link with a section's href steps into it. */
  sections?: ShellSection[]
  right?: ReactNode
  /** Marks the nav link whose href prefixes it as current. */
  currentPath?: string
}) {
  const scrolled = useScrolled()
  const [section, setOpen] = useState(() => sections.find((s) => currentPath?.startsWith(s.href)))
  const level = section ? (
      <>
        <a
          className="nav-back"
          href="/"
          aria-label="Main menu"
          title="Main menu"
          onClick={(e) => {
            e.preventDefault()
            setOpen(undefined)
          }}
        >
          <ChevronLeft size={14} strokeWidth={2} aria-hidden />
          <span className="nav-back-label">Main menu</span>
        </a>
        <a className="nav-section" href={section.href} aria-current={currentPath === section.href ? `page` : undefined}>
          {section.label}
        </a>
        {section.links.map((link) => (
          <a key={link.href} href={link.href} aria-current={isCurrent(link, currentPath) ? `page` : undefined}>
            {link.label}
          </a>
        ))}
      </>
    ) : (
      nav.map((link) => {
        const into = sections.find((s) => s.href === link.href)
        return (
          <a
            key={link.href}
            href={link.href}
            aria-current={isCurrent(link, currentPath) ? `page` : undefined}
            onClick={
              into
                ? (e) => {
                    e.preventDefault()
                    setOpen(into)
                  }
                : undefined
            }
          >
            {link.label}
          </a>
        )
      })
    )
  return (
    <header className={`topbar${scrolled ? ` is-scrolled` : ``}`}>
      <div className="shell topbar-inner">
        <a className="brand" href={homeHref}>
          <ExpLogo size={22} />
          <span>{brand}</span>
        </a>
        <nav className="nav">{level}</nav>
        <div className="topbar-right">
          {right}
          <PhoneMenu>{level}</PhoneMenu>
        </div>
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
