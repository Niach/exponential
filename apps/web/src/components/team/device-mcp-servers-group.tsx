// EXP-891: Settings › MCP servers — the "On your devices" groups. What each
// MACHINE connects on its own runs, beside the team registry above: one
// band per device (its label, its owner on a teammate's shared server, the
// online dot), flat rows under it (name, transport · target, where the row
// came from, Off when disabled). READ-ONLY here by design — the rows are
// written by the machines themselves (the desktop pane, `exponential mcp
// import` / `add`), and a machine is the only place that can see its own
// claude/codex config; the hand-off line says where to go.
import { useMemo } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import { conceptIcon, GlassRow, GlassSectionHeader, ListRow, Pill } from "@exp/ui"
import type { Device } from "@/db/schema"
import { deviceCollection } from "@/lib/collections"
import { deviceRowIsOnline } from "@/lib/steer-devices"
import {
  DEVICE_MCP_SETUP_HINT,
  deviceMcpServerSourceLabel,
  deviceMcpServerTarget,
  groupDeviceMcpServers,
  type DeviceMcpServerListRow,
} from "@/lib/mcp/device-mcp-servers"
import { MCP_TRANSPORT_LABELS } from "@/lib/mcp-servers"
import { useDeviceMcpServers } from "@/hooks/use-device-mcp-servers"
import { useNow } from "@/hooks/use-now"
import { cn } from "@/lib/utils"

const McpIcon = conceptIcon(`settings-mcp`)
const DeviceIcon = conceptIcon(`nav-devices`)

export function DeviceMcpServersGroup({
  teamId,
  currentUserId,
}: {
  teamId: string
  currentUserId: string
}) {
  const { rows, error } = useDeviceMcpServers(teamId)
  const now = useNow(30_000)
  const { data: deviceRows } = useLiveQuery((query) =>
    query.from({ d: deviceCollection })
  )
  const onlineByRowId = useMemo(() => {
    const map = new Map<string, boolean>()
    for (const device of (deviceRows ?? []) as Device[]) {
      map.set(
        device.id,
        device.lastSeenAt ? deviceRowIsOnline(device.lastSeenAt, now) : false
      )
    }
    return map
  }, [deviceRows, now])

  const groups = useMemo(
    () => (rows ? groupDeviceMcpServers(rows, currentUserId) : []),
    [rows, currentUserId]
  )

  return (
    <div className="mt-6 mb-6">
      <GlassSectionHeader label="On your devices" count={rows?.length} />
      <p className="mb-3 px-1 text-xs text-muted-foreground">{DEVICE_MCP_SETUP_HINT}</p>
      {error && <p className="px-1 pb-2 text-xs text-destructive">{error}</p>}
      {rows === null ? (
        <div className="px-1 py-3 text-sm text-muted-foreground">Loading…</div>
      ) : groups.length === 0 ? (
        <GlassRow className="text-sm text-muted-foreground">
          No machine has its own servers yet. Ran `claude mcp add` somewhere?
          Import it there and it shows up here.
        </GlassRow>
      ) : (
        <div className="flex flex-col gap-3">
          {groups.map((group) => {
            const mine = group.userId === currentUserId
            const online = onlineByRowId.get(group.deviceRowId) ?? false
            const label = group.deviceLabel || group.deviceId
            return (
              <div key={group.deviceRowId} data-testid="device-mcp-group">
                <GlassSectionHeader
                  label={mine ? label : `${label} · ${group.ownerName || `teammate`}`}
                  leading={
                    <span className="flex items-center gap-1.5">
                      <DeviceIcon className="size-3.5 shrink-0 text-muted-foreground" />
                      <span
                        className={cn(
                          `size-1.5 rounded-full`,
                          online ? `bg-emerald-500` : `bg-muted-foreground/40`
                        )}
                        aria-hidden
                      />
                    </span>
                  }
                  count={group.servers.length}
                />
                <div className="flex flex-col">
                  {group.servers.map((server) => (
                    <DeviceServerRow key={server.id} server={server} />
                  ))}
                </div>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}

function DeviceServerRow({ server }: { server: DeviceMcpServerListRow }) {
  const target = deviceMcpServerTarget(server)
  const transport = MCP_TRANSPORT_LABELS[server.transport] ?? server.transport
  return (
    <ListRow className="gap-3 py-2" title={target || undefined}>
      <McpIcon
        className={cn(
          `size-4 shrink-0`,
          server.enabled ? `text-foreground/70` : `text-muted-foreground/50`
        )}
      />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-baseline gap-1.5">
          <span
            className={cn(
              `min-w-0 truncate text-sm font-medium`,
              !server.enabled && `text-muted-foreground`
            )}
          >
            {server.name}
          </span>
          <Pill size="sm" title={server.source === `detected` ? `Imported from the machine's own agent config` : `Typed on the machine`}>
            {deviceMcpServerSourceLabel(server)}
          </Pill>
          {!server.enabled && (
            <Pill size="sm" title="Switched off on the machine; not connected on its runs">
              Off
            </Pill>
          )}
        </div>
        <div className="truncate text-xs text-muted-foreground">
          {`${transport} · ${target}`}
        </div>
      </div>
    </ListRow>
  )
}
