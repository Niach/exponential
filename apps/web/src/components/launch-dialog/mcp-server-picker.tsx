// EXP-792: the "MCP servers" multiselect every launch surface shares. The
// server holds each member's credential, so readiness is the CALLER's own
// `connection` — the same on every machine. A server they have not connected
// (or whose sign-in expired) is greyed with "Connect first" under its name;
// picking it opens Settings › MCP servers `?connect=<id>` in a new tab
// instead of adding it (the list refetches when this tab is visible again).
// Hidden by the caller when the team has no servers.
//
// The rows are `@exp/ui`'s `McpServerPicker` (the shared `Picker` in
// `mode="multi"`, each server by its glyph), the trigger the glass ladder's
// `row` like its sibling options.
import {
  McpServerPicker as McpServerPickerSurface,
  PickerItemBody,
  PickerMenuRows,
  PickerTrigger,
  mcpServerPickerItems,
  pickerSummary,
} from "@exp/ui"
import { mcpNotReadyLabel, type McpServerRow } from "@/lib/mcp-servers"
import { cn } from "@/lib/utils"

export function mcpPickSummary(
  servers: readonly Pick<McpServerRow, `id` | `name`>[],
  selectedIds: readonly string[]
): string {
  return pickerSummary(
    servers
      .filter((server) => selectedIds.includes(server.id))
      .map((server) => server.name),
    `None`
  )
}

type McpPickerServerRow = Pick<
  McpServerRow,
  `id` | `name` | `url` | `command` | `connection`
>

/** The toggle a pick reports: the added or removed id, or — for a server the
 *  caller has not connected — nothing, after opening its connect link. */
function resolveMcpToggle(
  next: readonly string[],
  selectedIds: readonly string[],
  notes: ReadonlyMap<string, string | null>,
  connectHref: (serverId: string) => string | null
): string | null {
  const added = next.find((id) => !selectedIds.includes(id))
  const removed = selectedIds.find((id) => !next.includes(id))
  if (added && notes.get(added)) {
    // Not usable yet: connecting is the only way forward.
    const href = connectHref(added)
    if (href) window.open(href, `_blank`, `noopener`)
    return null
  }
  return added ?? removed ?? null
}

function mcpNotes(servers: readonly McpPickerServerRow[]) {
  return new Map(servers.map((server) => [server.id, mcpNotReadyLabel(server)]))
}

function McpRowBody({
  item,
  note,
}: {
  item: Parameters<typeof PickerItemBody>[0][`item`]
  note: string | null
}) {
  return (
    <span
      data-mcp-ready={note ? `false` : `true`}
      title={note ? `Opens Settings › MCP servers to connect` : undefined}
      className={cn(`flex min-w-0 flex-1 items-center gap-2.5`, note && `opacity-50`)}
    >
      <PickerItemBody item={item} />
    </span>
  )
}

/** EXP-1249: the same servers as rows INSIDE a menu (the composer's "+" ›
 *  MCP servers): the picker's multi language, the menu stays open across
 *  toggles, an unconnected server opens its connect link instead. */
export function McpServerMenuRows({
  servers,
  selectedIds,
  onToggle,
  connectHref,
}: {
  servers: readonly McpPickerServerRow[]
  selectedIds: readonly string[]
  onToggle: (id: string) => void
  connectHref: (serverId: string) => string | null
}) {
  const notes = mcpNotes(servers)
  return (
    <PickerMenuRows
      mode="multi"
      items={mcpServerPickerItems(
        servers.map((server) => ({
          id: server.id,
          name: server.name,
          url: server.url,
          command: server.command,
          description: notes.get(server.id) ?? undefined,
        }))
      )}
      value={selectedIds}
      onChange={(next) => {
        const changed = resolveMcpToggle(next, selectedIds, notes, connectHref)
        if (changed) onToggle(changed)
      }}
      renderItem={(item) => (
        <McpRowBody item={item} note={notes.get(item.value) ?? null} />
      )}
    />
  )
}

export function McpServerPicker({
  servers,
  selectedIds,
  onToggle,
  connectHref,
  disabled,
}: {
  servers: readonly McpPickerServerRow[]
  selectedIds: readonly string[]
  onToggle: (id: string) => void
  /** The settings deep link that connects `serverId` (null = no link). */
  connectHref: (serverId: string) => string | null
  disabled?: boolean
}) {
  const notes = mcpNotes(servers)

  return (
    <McpServerPickerSurface
      servers={servers.map((server) => ({
        id: server.id,
        name: server.name,
        url: server.url,
        command: server.command,
        description: notes.get(server.id) ?? undefined,
      }))}
      value={selectedIds}
      // The primitive hands back the whole next selection; this picker's hosts
      // own one id at a time, so the change is reported as the toggled row.
      onChange={(next) => {
        const changed = resolveMcpToggle(next, selectedIds, notes, connectHref)
        if (changed) onToggle(changed)
      }}
      disabled={disabled}
      search={servers.length > 6}
      width="md"
      renderItem={(item) => (
        <McpRowBody item={item} note={notes.get(item.value) ?? null} />
      )}
      trigger={
        <PickerTrigger
          variant="row"
          label="MCP servers"
          value={
            selectedIds.length > 0
              ? mcpPickSummary(servers, selectedIds)
              : undefined
          }
          placeholder="None"
          disabled={disabled}
        />
      }
    />
  )
}
