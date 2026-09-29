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
  PickerTrigger,
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

export function McpServerPicker({
  servers,
  selectedIds,
  onToggle,
  connectHref,
  disabled,
}: {
  servers: readonly Pick<
    McpServerRow,
    `id` | `name` | `url` | `command` | `connection`
  >[]
  selectedIds: readonly string[]
  onToggle: (id: string) => void
  /** The settings deep link that connects `serverId` (null = no link). */
  connectHref: (serverId: string) => string | null
  disabled?: boolean
}) {
  const notes = new Map(
    servers.map((server) => [server.id, mcpNotReadyLabel(server)])
  )

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
        const added = next.find((id) => !selectedIds.includes(id))
        const removed = selectedIds.find((id) => !next.includes(id))
        if (added && notes.get(added)) {
          // Not usable yet: connecting is the only way forward.
          const href = connectHref(added)
          if (href) window.open(href, `_blank`, `noopener`)
          return
        }
        const changed = added ?? removed
        if (changed) onToggle(changed)
      }}
      disabled={disabled}
      search={servers.length > 6}
      width="md"
      renderItem={(item) => {
        const note = notes.get(item.value) ?? null
        return (
          <span
            data-mcp-ready={note ? `false` : `true`}
            title={note ? `Opens Settings › MCP servers to connect` : undefined}
            className={cn(
              `flex min-w-0 flex-1 items-center gap-2.5`,
              note && `opacity-50`
            )}
          >
            <PickerItemBody item={item} />
          </span>
        )
      }}
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
