/* The library family (EXP-1421, docs/libraries.md): the five open libs
   Exponential is built from, in nav order. The marketing front page's
   library section, the footer, every lib's docs sidebar and the header
   sections all read this one list, so the libs always link each other. */
import type { DocsNavEntry, ShellSection } from "@exp/site-shell"
import type { Status } from "../../../src/components/blog/StatusChip"

export type Library = {
  name: string
  href: string
  /** One line, what the lib does. */
  desc: string
  status: Status
  /** The chip's word (published, beta, planned …). */
  label: string
}

export const LIBRARIES: readonly Library[] = [
  { name: `eUI`, href: `/ui/`, desc: `Generative UI: an agent sends A2UI, native renderers paint it on web, iOS, Android and desktop.`, status: `shipped`, label: `published` },
  { name: `eTalk`, href: `/talk/`, desc: `Speech in and out for agents: push to talk and spoken replies on your own GPU.`, status: `in-progress`, label: `beta` },
  { name: `ePilot`, href: `/pilot/`, desc: `Computer use: the agent plans, acts on the real screen, and looks again.`, status: `in-progress`, label: `in the app` },
  { name: `eNet`, href: `/net/`, desc: `Peer to peer and end-to-end encrypted tunnels between every client, on iroh.`, status: `in-progress`, label: `in development` },
  { name: `eApps`, href: `/apps/`, desc: `The app package format: templates, bindings, records and tools as data, never code.`, status: `in-progress`, label: `in the app` },
]

/** A one-page lib's header level: its name, no sub-links. */
export const libSection = (name: string): ShellSection => {
  const lib = LIBRARIES.find((l) => l.name === name)!
  return { label: lib.name, href: lib.href, links: [] }
}

/** The docs sidebar of a one-page lib: the whole family, in order. */
export const LIBRARIES_NAV: DocsNavEntry[] = LIBRARIES.map((l) => ({ path: l.href, label: l.name }))

/** The other libs as a sidebar group, under a lib's own pages. */
export const otherLibrariesNav = (name: string): DocsNavEntry[] =>
  LIBRARIES.filter((l) => l.name !== name).map((l) => ({ path: l.href, label: l.name, group: `Libraries` }))
