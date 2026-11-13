/* The site's map: the header's two menus (Product, SDK), its plain links,
   the footer's columns and the home page's SDK wall all read it. A menu
   item = an icon and a name; the featured card = a picture and a name. */
import type { LucideIcon } from "lucide-react"
import { AppWindow, Bot, Brain, Cpu, Download, GitBranch, LayoutTemplate, Mic, Monitor, MousePointerClick, Network, Plug, Server, Sparkles } from "lucide-react"
import { LIBRARIES } from "../../lib/libraries.ts"
import { LINKS } from "./links.ts"

export interface NavItem {
  label: string
  href: string
  icon?: LucideIcon
}
export interface NavMenu {
  label: string
  items: NavItem[]
  featured: { label: string; href: string; src: string; alt: string }
}

const MODULE_ICON: Record<string, LucideIcon> = {
  genui: LayoutTemplate,
  motion: Sparkles,
  voice: Mic,
  p2p: Network,
  "computer-use": MousePointerClick,
  "mcp-proxy": Plug,
  agents: Brain,
  core: Cpu,
  code: GitBranch,
}

/** The Exponential SDK: every module's product page, Apps last. */
export const SDK: NavItem[] = [
  ...LIBRARIES.map((lib) => ({ label: lib.name, href: lib.product, icon: MODULE_ICON[lib.slug] })),
  { label: `Apps`, href: `/sdk/apps/`, icon: AppWindow },
]

export const MENUS: NavMenu[] = [
  {
    label: `Product`,
    items: [
      { label: `Apps`, href: `/sdk/apps/`, icon: AppWindow },
      { label: `Agents`, href: `/sdk/agents/`, icon: Bot },
      { label: `Exponential Desktop`, href: `/desktop/`, icon: Monitor },
      { label: `Download`, href: `/download/`, icon: Download },
      { label: `Self-host`, href: `/docs/get-started/self-host/`, icon: Server },
    ],
    featured: { label: `Apps`, href: `/sdk/apps/`, src: `/eapps/time-tracker.webp`, alt: `Hours, a team app: a month of hours as a chart and a table` },
  },
  {
    label: `SDK`,
    items: SDK,
    featured: { label: `genUI`, href: `/sdk/genui/`, src: `/eapps/admin-console.webp`, alt: `An admin console painted by genUI` },
  },
]

export const LINKS_NAV: NavItem[] = [
  { label: `Docs`, href: `/docs/` },
  { label: `Pricing`, href: `/pricing/` },
  { label: `Blog`, href: `/blog/` },
]

export const SIGN_UP = { label: `Get started`, href: LINKS.app.login }

export const FOOTER: { title: string; items: NavItem[] }[] = [
  { title: `Product`, items: [...MENUS[0]!.items, { label: `Pricing`, href: `/pricing/` }] },
  { title: `SDK`, items: SDK },
  {
    title: `Resources`,
    items: [
      { label: `Docs`, href: `/docs/` },
      { label: `Blog`, href: `/blog/` },
      { label: `RSS`, href: `/blog/rss.xml` },
    ],
  },
  {
    title: `Company`,
    items: [
      { label: `GitHub`, href: LINKS.github.repo },
      { label: `Contact`, href: `/contact/` },
      { label: `Imprint`, href: `/imprint/` },
    ],
  },
]
