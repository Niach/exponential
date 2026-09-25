// EXP-792: the "MCP servers" multiselect every launch surface shares. A row
// the chosen device is NOT ready for (no OAuth sign-in or typed secret on that
// machine, per the readiness matrix) is greyed with the reason UNDER its name;
// picking it anyway is allowed — it is never `disabled` — the desktop launcher
// then names the blocker. Hidden by the caller when the team has no servers.
//
// EXP-1030: the rows are the shared `Picker` primitive's (EXP-1021) in
// `mode="multi"` — the pick reads as the row's own highlight, like every
// other multi picker — and the reason rides the primitive's muted second line
// (`description`), which is exactly how the desktop's own MCP rows draw it
// (`launch_options.rs`: greyed, reason under the name, never unpickable).
import {
  Picker,
  PickerItemBody,
  Pill,
  conceptIcon,
  type PickerItem,
} from "@exp/ui"
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
  const items: PickerItem[] = servers.map((server) => ({
    value: server.id,
    label: server.name,
    description: reasons.get(server.id) ?? undefined,
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
        const changed = added ?? removed
        if (changed) onToggle(changed)
      }}
      disabled={disabled}
      search={servers.length > 6}
      searchPlaceholder="Filter servers…"
      emptyText="No servers found."
      width="md"
      mobileTitle="MCP servers"
      // The row BODY is the primitive's; only the GREYING is this picker's —
      // a blocked row still toggles, it just reads as the one that will need
      // a fix on the machine first.
      renderItem={(item) => {
        const reason = reasons.get(item.value) ?? null
        return (
          <span
            title={reason ?? undefined}
            className={cn(
              `flex min-w-0 flex-1 items-center gap-2.5`,
              reason && `opacity-50`
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
