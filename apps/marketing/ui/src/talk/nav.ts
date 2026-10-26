/* The /talk/ level of the site header and the Talk docs' sidebar (NEW-15):
   the /ui/ section's twin (ui/src/lib/nav.ts). */
import type { DocsNavEntry, ShellSection } from "@exp/site-shell"

export const TALK_HOME = `/talk/`

export const TALK_NAV = [
  { label: `How it works`, href: `/talk/how-it-works/` },
  { label: `Install`, href: `/talk/install/` },
  { label: `Guides`, href: `/talk/guides/` },
  { label: `API`, href: `/talk/api/` },
  { label: `Numbers`, href: `/talk/numbers/` },
  { label: `Licences`, href: `/talk/licences/` },
] as const

export const TALK_SECTION: ShellSection = { label: `Talk`, href: TALK_HOME, links: [...TALK_NAV] }

/** The docs sidebar of every Talk page: the reading order. */
export const TALK_DOCS_NAV: DocsNavEntry[] = [
  { path: TALK_HOME, label: `Overview` },
  ...TALK_NAV.map((l) => ({ path: l.href, label: l.label })),
]

/** The guides, in nav order (each one page under /talk/guides/). */
export const TALK_GUIDES = [
  { slug: `your-voice`, title: `Your voice`, blurb: `Record a reference clip and write its transcript.` },
  { slug: `push-to-talk`, title: `Push-to-talk shortcuts`, blurb: `Which keys work where: X11, Wayland, macOS, Windows.` },
] as const

export const talkGuidePath = (slug: string) => `/talk/guides/${slug}/`
