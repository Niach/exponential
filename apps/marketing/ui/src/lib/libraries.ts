/* The library family (EXP-1421, docs/libraries.md): the open libs
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
  { name: `eUI`, href: `/ui/`, desc: `Generative UI: an agent sends A2UI, native renderers paint it on web, iOS, Android and desktop.`, status: `shipped`, label: `0.3.0` },
  { name: `eTalk`, href: `/talk/`, desc: `Speech in and out for agents: push to talk, spoken replies and calls on your own GPU.`, status: `in-progress`, label: `0.2.3 beta` },
  { name: `ePilot`, href: `/pilot/`, desc: `Computer use: the agent plans, acts on the real screen, and looks again.`, status: `shipped`, label: `0.1.0` },
  { name: `eNet`, href: `/net/`, desc: `Peer to peer and end-to-end encrypted tunnels between every client, on iroh.`, status: `shipped`, label: `0.1.0` },
  { name: `eApps`, href: `/apps/`, desc: `The app package format: templates, bindings, records and MCP tools as data, never code.`, status: `in-progress`, label: `in eUI` },
  { name: `eMCP`, href: `/mcp/`, desc: `MCP for devices: a loopback server for your tools, a client with OAuth and a vault.`, status: `shipped`, label: `0.1.0` },
  { name: `eAgent`, href: `/agent/`, desc: `Runs your own Claude Code or Codex over ACP: one local account, no server.`, status: `in-progress`, label: `0.1.0` },
  { name: `eCore`, href: `/core/`, desc: `The plain types the modules share: ids, grants, verdicts, errors and the event envelope.`, status: `in-progress`, label: `built` },
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
