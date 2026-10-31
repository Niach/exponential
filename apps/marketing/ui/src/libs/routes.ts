/* EXP-1421: the one-page libs (ePilot, eNet, eApps, eMCP), each a docs page with
   the library family as its sidebar. ui/src/lib/routes.ts appends these to
   ROUTES like the Talk section's. */
import type { Route } from "../lib/routes"

const lib = (path: string, load: Route[`load`], title: string, description: string, sources: string[]): Route => ({
  path,
  htmlFile: `${path.slice(1)}index.html`,
  load,
  title,
  description,
  sources,
  ogImage: `/og/og-ui.png`,
})

export const LIB_ROUTES: readonly Route[] = [
  lib(
    `/pilot/`,
    () => import("./Pilot"),
    `ePilot · computer use for agents`,
    `ePilot lets an agent see the real screen and drive the real pointer and keyboard: screenshot, act, look again. Foreground only, Esc to abort, screen memory. In the desktop app; the epilot crates as a git dependency.`,
    [`ui/src/libs/Pilot.tsx`, `../desktop/crates/epilot`, `../desktop/crates/epilot-screen`]
  ),
  lib(
    `/net/`,
    () => import("./Net"),
    `eNet · peer to peer, end-to-end encrypted`,
    `eNet connects every Exponential client directly and end to end encrypted on iroh, with a relay that can't read and a relay-only browser build. Chat, files, records, calls and sealed push. Not released yet.`,
    [`ui/src/libs/Net.tsx`, `../desktop/crates/enet-core`]
  ),
  lib(
    `/apps/`,
    () => import("./Apps"),
    `eApps · the app package format`,
    `eApps is one package format for apps an agent builds: a manifest, eUI templates, bindings, a function allowlist and records, data never code, with tools, extensions and signed package files (format v2).`,
    [`ui/src/libs/Apps.tsx`, `../../packages/exponential-ui/src/host/package.ts`]
  ),
  lib(
    `/mcp/`,
    () => import("./Mcp"),
    `eMCP · MCP for devices`,
    `eMCP serves your own tools on a loopback MCP server, calls any MCP server, and keeps OAuth credentials in a vault you provide. On rmcp; the emcp crate as a git dependency.`,
    [`ui/src/libs/Mcp.tsx`, `../desktop/crates/emcp`, `../../packages/emcp`]
  ),
]
