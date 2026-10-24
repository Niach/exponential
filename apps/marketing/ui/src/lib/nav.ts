/* The /ui/ level of the site header (one header for all of exponential.at:
   the main menu's UI SDK link steps into it, the back button steps out). */
import type { ShellSection } from "@exp/site-shell"

export const NAV = [
  { label: `Concepts`, href: `/ui/concepts/` },
  { label: `Components`, href: `/ui/components/` },
  { label: `Themes`, href: `/ui/themes/` },
  { label: `Guides`, href: `/ui/guides/` },
  { label: `Playground`, href: `/ui/playground/` },
  { label: `Conformance`, href: `/ui/conformance/` },
  { label: `Eval`, href: `/ui/eval/` },
  { label: `Numbers`, href: `/ui/numbers/` },
] as const

export const UI_SECTION: ShellSection = { label: `UI SDK`, href: `/ui/`, links: [...NAV] }
