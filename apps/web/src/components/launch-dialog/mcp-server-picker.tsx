// EXP-792: the "MCP servers" multiselect every launch surface shares — the
// shared `Combobox` (EXP-941) over the team's server list. A row the chosen
// device is NOT ready for (no OAuth sign-in or typed secret on that machine,
// per the readiness matrix) is greyed with the reason as a tooltip; picking it
// anyway is allowed — it is never `disabled` — the desktop launcher then names
// the blocker. Hidden by the caller when the team has no servers at all.
import { Combobox, Pill, conceptIcon, type PickerOption } from "@exp/ui"
import { serverBlockReason, type McpServerRow } from "@/lib/mcp-servers"
import type { SteerDevice } from "@/lib/steer-devices"
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
  device,
  now,
  disabled,
  renderTrigger,
}: {
  servers: readonly McpServerRow[]
  selectedIds: readonly string[]
  onToggle: (id: string) => void
  /** The machine the run lands on — decides which rows read "not ready". */
  device: SteerDevice | undefined
  now: Date
  disabled?: boolean
  /** Replaces the default pill (a settings row renders its own value). */
  renderTrigger?: (summary: string) => React.ReactNode
}) {
  const summary = mcpPickSummary(servers, selectedIds)
  const reasons = new Map(
    servers.map((server) => [
      server.id,
      serverBlockReason(
        server,
        device
          ? { deviceId: device.deviceId, deviceLabel: device.deviceLabel }
          : null,
        now
      ),
    ])
  )
  const options: PickerOption[] = servers.map((server) => ({
    value: server.id,
    label: server.name,
  }))

  return (
    <Combobox
      multiple
      options={options}
      value={selectedIds}
      // The primitive hands back the whole next selection; this picker's hosts
      // own one id at a time, so the change is reported as the toggled row.
      onChange={(next) => {
        const added = next.find((id) => !selectedIds.includes(id))
        const removed = selectedIds.find((id) => !next.includes(id))
        const changed = added ?? removed
        if (changed) onToggle(changed)
      }}
      disabled={disabled}
      searchable={servers.length > 6}
      placeholder="Filter servers…"
      emptyText="No servers found."
      width="md"
      mobileTitle="MCP servers"
      renderOption={(option) => {
        const reason = reasons.get(option.value) ?? null
        return (
          <span
            title={reason ?? undefined}
            className={cn(
              `flex min-w-0 flex-1 items-center gap-2`,
              reason && `opacity-50`
            )}
          >
            <span className="min-w-0 flex-1 truncate text-sm">
              {option.label}
            </span>
            {reason && (
              <span className="shrink-0 text-[10px] text-muted-foreground">
                Not ready
              </span>
            )}
          </span>
        )
      }}
      renderTrigger={() =>
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
