/* The /ui/ pages wear exponential.at's ONE header and footer
   (src/components/SiteShell.tsx); the header opens on the UI SDK level.
   The light/dark switch is this section's own (only these pages have a
   light scheme). */
import type { ReactNode } from "react"
import { Moon, Sun } from "lucide-react"
import { SiteFooter, SiteHeader } from "../../../src/components/SiteShell"
import { currentScheme, setScheme, useScheme } from "../lib/scheme"

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
      <SiteHeader currentPath={path} extra={<SchemeToggle />} />
      <main>{children}</main>
      <SiteFooter />
    </>
  )
}
