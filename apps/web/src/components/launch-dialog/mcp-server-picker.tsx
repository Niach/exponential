// EXP-792: the "MCP servers" multiselect every launch surface shares — the
// label picker's checkbox-row popover over the team's server list. A row the
// chosen device is NOT ready for (no OAuth sign-in or typed secret on that
// machine, per the readiness matrix) is greyed with the reason as a tooltip;
// picking it anyway is allowed, the desktop launcher then names the blocker.
// Hidden by the caller when the team has no servers at all.
import { Checkbox } from "@/components/ui/checkbox"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import {
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
} from "@/components/mobile-popover"
import { Pill } from "@/components/ui/pill"
import { conceptIcon } from "@/lib/icons.generated"
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
  return (
    <MobilePopover>
      <MobilePopoverTrigger asChild disabled={disabled}>
        {renderTrigger ? (
          renderTrigger(summary)
        ) : (
          <Pill mode="action" disabled={disabled}>
            <McpIcon className="size-3" />
            <span className="max-w-[9rem] truncate">{summary}</span>
          </Pill>
        )}
      </MobilePopoverTrigger>
      <MobilePopoverContent
        className="w-[16rem] p-0"
        align="start"
        mobileTitle="MCP servers"
      >
        <Command>
          {servers.length > 6 && <CommandInput placeholder="Filter servers…" />}
          <CommandList>
            <CommandEmpty>No servers found.</CommandEmpty>
            <CommandGroup>
              {servers.map((server) => {
                const selected = selectedIds.includes(server.id)
                const reason = serverBlockReason(
                  server,
                  device
                    ? { deviceId: device.deviceId, deviceLabel: device.deviceLabel }
                    : null,
                  now
                )
                return (
                  <CommandItem
                    key={server.id}
                    value={server.id}
                    keywords={[server.name]}
                    onSelect={() => onToggle(server.id)}
                    title={reason ?? undefined}
                    className={cn(
                      `flex items-center gap-2`,
                      reason && `opacity-50`
                    )}
                  >
                    <Checkbox checked={selected} className="pointer-events-none" />
                    <span className="min-w-0 flex-1 truncate text-sm">
                      {server.name}
                    </span>
                    {reason && (
                      <span className="shrink-0 text-[10px] text-muted-foreground">
                        Not ready
                      </span>
                    )}
                  </CommandItem>
                )
              })}
            </CommandGroup>
          </CommandList>
        </Command>
      </MobilePopoverContent>
    </MobilePopover>
  )
}
