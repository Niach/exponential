import type { ReactNode } from "react"
import { isBrandIcon, type BrandIconName } from "@exp/icons"
import { contract } from "@exp/domain-contract"

import { brandIcon } from "../brand-icons.generated"
import { conceptIcon } from "../icons.generated"
import {
  Picker,
  type PickerGlyph,
  type PickerItem,
  type PickerSurfaceProps,
} from "./picker"

// EXP-792 — the MCP server picker: team MCP servers by mark + name, the host
// (or the command) as the muted second line. The launch composers pick the
// servers a run connects to (multi); the Add dialog lists the same rows for
// the well-known hosted servers through `PickerList` (the dialog is already
// a surface). A server wears the REAL brand mark of the service it belongs
// to when its host is in the catalog (contract `mcpCatalog` → the generated
// brand set, packages/icons `brand`), a terminal for a command, the plug for
// anything else. Nobody draws a brand path by hand: the marks are vendored
// selfh.st light SVGs, reproduced as-is.

export interface McpCatalogEntry {
  id: string
  name: string
  /** The endpoint, or a TEMPLATE containing `{host}` (self-managed installs). */
  url: string
  mark: BrandIconName
  /** `url` is a template: never probed, never "Added". */
  template: boolean
}

/** Well-known hosted MCP servers the Add dialog offers by name — the
 * contract's table, typed. Dropped for lack of a selfh.st LIGHT mark (add
 * one to icons.json `brand` and the row to contract.json once it exists):
 * Vercel, Asana, Canva, Clerk, ClickUp, HubSpot, Hugging Face, Intercom,
 * Miro, Mixpanel, monday, Neon, PagerDuty, Railway, Render, Resend, WorkOS,
 * Zapier, Context7, DeepWiki. */
export const MCP_CATALOG: readonly McpCatalogEntry[] =
  contract.mcpCatalog.servers.map((entry) => {
    if (!isBrandIcon(entry.mark)) {
      throw new Error(
        `mcpCatalog "${entry.id}" names mark "${entry.mark}", which icons.json does not vendor`
      )
    }
    return {
      id: entry.id,
      name: entry.name,
      url: entry.url,
      mark: entry.mark,
      template: isMcpCatalogTemplate(entry.url),
    }
  })

/** A catalog URL with a `{host}` placeholder: the owner fills the host in. */
export function isMcpCatalogTemplate(url: string): boolean {
  return url.includes(`{host}`)
}

function hostOf(url: string | null | undefined): string | null {
  if (!url) return null
  try {
    return new URL(url.trim()).host.toLowerCase()
  } catch {
    return null
  }
}

/** The path a template pins (`https://{host}/api/v4/mcp` → `/api/v4/mcp`). */
function templatePath(template: string): string | null {
  try {
    return new URL(template.replace(`{host}`, `template.invalid`)).pathname
  } catch {
    return null
  }
}

/** The catalog entry a server's URL belongs to: a hosted entry by exact
 * host, else a template whose path the URL carries (a self-managed GitLab
 * at any host). A team row added from the catalog keeps its mark this way. */
export function mcpCatalogEntryFor(
  url: string | null | undefined
): McpCatalogEntry | undefined {
  const host = hostOf(url)
  if (!host) return undefined
  const hosted = MCP_CATALOG.find(
    (entry) => !entry.template && hostOf(entry.url) === host
  )
  if (hosted) return hosted
  let path: string
  try {
    path = new URL(url!.trim()).pathname
  } catch {
    return undefined
  }
  return MCP_CATALOG.find(
    (entry) => entry.template && templatePath(entry.url) === path
  )
}

/** A server's glyph: its catalog entry's brand mark (matched by URL, so a
 * team row added from the catalog keeps it), a terminal for a command, else
 * the plug. */
export function getMcpServerIcon(server: {
  url?: string | null
  command?: string | null
}): PickerGlyph {
  const entry = mcpCatalogEntryFor(server.url)
  if (entry) return brandIcon(entry.mark)
  return conceptIcon(server.command && !server.url ? `session-shell` : `ui-mcp`)
}

export interface McpPickerServer {
  id: string
  name: string
  url?: string | null
  command?: string | null
  /** The muted second line; absent = the host (or the command). */
  description?: ReactNode
  disabled?: boolean
}

export function mcpServerPickerItems(
  servers: readonly McpPickerServer[]
): PickerItem[] {
  return servers.map((server) => {
    const target = hostOf(server.url) ?? server.command ?? undefined
    return {
      value: server.id,
      label: server.name,
      icon: getMcpServerIcon(server),
      description: server.description ?? target,
      disabled: server.disabled,
      keywords: [server.name, ...(target ? [target] : [])],
    }
  })
}

export interface McpServerPickerProps extends PickerSurfaceProps {
  servers: readonly McpPickerServer[]
  value: readonly string[]
  onChange: (serverIds: string[]) => void
  trigger: ReactNode
  /** Replaces the row BODY (a host dims the ones it cannot use yet). */
  renderItem?: (item: PickerItem, state: { selected: boolean }) => ReactNode
  mobileTitle?: string
  search?: boolean
  disabled?: boolean
  emptyText?: string
  className?: string
}

export function McpServerPicker({
  servers,
  emptyText = `No servers found.`,
  mobileTitle = `MCP servers`,
  searchPlaceholder = `Search servers…`,
  search = true,
  ...props
}: McpServerPickerProps) {
  return (
    <Picker
      mode="multi"
      items={mcpServerPickerItems(servers)}
      emptyText={emptyText}
      mobileTitle={mobileTitle}
      searchPlaceholder={searchPlaceholder}
      search={search}
      {...props}
    />
  )
}
