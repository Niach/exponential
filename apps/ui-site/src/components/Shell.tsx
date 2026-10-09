/* The site chrome: the header and footer bars shared with exponential.at
   (@exp/site-shell), bound to ui.exponential.at's nav. */
import { useEffect, useRef, type ReactNode } from "react"
import { Menu, Moon, Sun } from "lucide-react"
import { GitHubStarsButton, SiteFooterBar, SiteHeaderBar } from "@exp/site-shell"
import { NAV } from "../lib/nav"
import { currentScheme, setScheme, useScheme } from "../lib/scheme"
import { LINKS } from "../lib/site"

/** The nav on phones (the top bar's links hide at <= 720px); works without
 *  JS, and with it Escape or a tap outside closes it. */
function PhoneMenu({ path }: { path: string }) {
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
      <nav aria-label="Site">
        {NAV.map((l) => (
          <a key={l.href} href={l.href} aria-current={path.startsWith(l.href) ? `page` : undefined}>
            {l.label}
          </a>
        ))}
      </nav>
    </details>
  )
}

function SchemeToggle() {
  const scheme = useScheme()
  return (
    <button type="button" className="btn btn-ghost btn-sm site-icon-btn scheme-toggle" aria-label="Light mode" aria-pressed={scheme === `light`} title="Light / dark" onClick={() => setScheme(currentScheme() === `light` ? `dark` : `light`)}>
      <Sun className="is-sun" size={14} strokeWidth={1.8} aria-hidden />
      <Moon className="is-moon" size={14} strokeWidth={1.8} aria-hidden />
    </button>
  )
}

export function Shell({ path, children }: { path: string; children: ReactNode }) {
  return (
    <>
      <SiteHeaderBar
        brand="Exponential UI"
        nav={[...NAV]}
        currentPath={path}
        right={
          <>
            <SchemeToggle />
            <GitHubStarsButton variant="compact" />
            <a className="btn btn-ghost btn-sm" href={LINKS.marketing}>
              exponential.at
            </a>
            <a className="btn btn-primary btn-sm" href="/guides/react/">
              Get started
            </a>
            <PhoneMenu path={path} />
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
