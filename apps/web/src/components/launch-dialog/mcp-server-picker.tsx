// EXP-792: the "MCP servers" multiselect every launch surface shares. The
// server holds each member's credential, so readiness is the CALLER's own
// `connection` — the same on every machine. A server they have not connected
// (or whose sign-in expired) is greyed with "Connect first" under its name;
// picking it opens Settings › MCP servers `?connect=<id>` in a new tab
// instead of adding it (the list refetches when this tab is visible again).
// Hidden by the caller when the team has no servers.
//
// EXP-1030: the rows are the shared `Picker` primitive's (EXP-1021) in
// `mode="multi"` — the pick reads as the row's own highlight — and the note
// rides the primitive's muted second line (`description`).
import {
  Picker,
  PickerItemBody,
  Pill,
  conceptIcon,
  type PickerItem,
} from "@exp/ui"
import { mcpNotReadyLabel, type McpServerRow } from "@/lib/mcp-servers"
import { cn } from "@/lib/utils"

const McpIcon = conceptIcon(`settings-mcp`)

export function mcpPickSummary(
  servers: readonly Pick<McpServerRow, `id` | `name`>[],
  selectedIds: readonly string[]
): string {
  const names = servers
    .filter((server) => selectedIds.includes(server.id))
    .map((server) => server.name)
  if (names.length === 0) return `None`
  if (names.length <= 2) return names.join(`, `)
  return `${names[0]}, ${names[1]} +${names.length - 2}`
}

export function McpServerPicker({
  servers,
  selectedIds,
  onToggle,
  connectHref,
  disabled,
  renderTrigger,
}: {
  servers: readonly Pick<McpServerRow, `id` | `name` | `connection`>[]
  selectedIds: readonly string[]
  onToggle: (id: string) => void
  /** The settings deep link that connects `serverId` (null = no link). */
  connectHref: (serverId: string) => string | null
  disabled?: boolean
  /** Replaces the default pill (a settings row renders its own value). */
  renderTrigger?: (summary: string) => React.ReactNode
}) {
  const summary = mcpPickSummary(servers, selectedIds)
  const notes = new Map(
    servers.map((server) => [server.id, mcpNotReadyLabel(server)])
  )
  const items: PickerItem[] = servers.map((server) => ({
    value: server.id,
    label: server.name,
    description: notes.get(server.id) ?? undefined,
  }))

  return (
    <Picker
      mode="multi"
      items={items}
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
      searchPlaceholder="Filter servers…"
      emptyText="No servers found."
      width="md"
      mobileTitle="MCP servers"
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
        renderTrigger ? (
          renderTrigger(summary)
        ) : (
          <Pill mode="action" disabled={disabled}>
            <McpIcon className="size-3" />
            <span className="max-w-[9rem] truncate">{summary}</span>
          </Pill>
        )
      }
    />
  )
}
