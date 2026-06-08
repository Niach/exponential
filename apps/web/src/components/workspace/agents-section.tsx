import { useCallback, useEffect, useState } from "react"
import { Bot, Loader2, Monitor, Trash2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"

type DeviceList = Awaited<ReturnType<typeof trpc.agent.list.query>>[`agents`]

function formatSeen(value: Date | string | null): string {
  if (!value) return `Never seen`
  const date = value instanceof Date ? value : new Date(value)
  return `Last seen ${date.toLocaleString()}`
}

export function WorkspaceAgentsSection({
  workspaceId,
}: {
  workspaceId: string
}) {
  const [devices, setDevices] = useState<DeviceList>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [busyId, setBusyId] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      const result = await trpc.agent.list.query({ workspaceId })
      setDevices(result.agents)
      setError(null)
    } catch {
      // A FORBIDDEN/NOT_FOUND (e.g. non-owner) used to reject silently and
      // leave the panel stuck on the loading spinner. Surface it instead.
      setError(`Couldn't load devices — are you the owner of this workspace?`)
    } finally {
      setLoading(false)
    }
  }, [workspaceId])

  useEffect(() => {
    setLoading(true)
    void refresh()
  }, [refresh])

  const revokeDevice = async (agentId: string) => {
    if (
      !window.confirm(
        `Remove this device? Its credential is revoked and it stops running agents on all your workspaces until it re-registers.`
      )
    ) {
      return
    }
    setBusyId(agentId)
    try {
      await trpc.agent.revoke.mutate({ agentId })
      await refresh()
    } finally {
      setBusyId(null)
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <Monitor className="h-4 w-4" />
          Desktop devices
        </CardTitle>
        <CardDescription>
          Each Mac or PC you sign in to with the Exponential desktop app
          registers itself here automatically. A device can run coding agents on
          any issue assigned to it across your workspaces — no setup tokens.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex items-start gap-2 rounded-md border border-dashed px-3 py-3 text-sm text-muted-foreground">
          <Bot className="mt-0.5 h-4 w-4 shrink-0" />
          <span>
            Devices register from the desktop app (Linux & macOS) on sign-in.
            They appear here, where you can revoke any you no longer use.
          </span>
        </div>

        <div className="space-y-2">
          {loading ? (
            <div className="flex items-center gap-2 rounded-md border px-3 py-2 text-sm text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin" />
              Loading devices
            </div>
          ) : error ? (
            <div className="rounded-md border border-destructive/50 px-3 py-2 text-sm text-destructive">
              {error}
            </div>
          ) : devices.length === 0 ? (
            <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
              No devices registered yet.
            </div>
          ) : (
            devices.map((device) => (
              <div
                key={device.id}
                className="flex flex-col gap-3 overflow-hidden rounded-md border px-3 py-3 sm:flex-row sm:items-center sm:justify-between"
              >
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="break-all text-sm font-medium">
                      {device.name}
                    </span>
                    <Badge variant="secondary">device</Badge>
                  </div>
                  <div className="mt-1 truncate text-xs text-muted-foreground">
                    {device.ownerName ? `Owned by ${device.ownerName} · ` : ``}
                    {formatSeen(device.lastSeenAt)}
                  </div>
                </div>
                <div className="flex shrink-0 items-center gap-1">
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => revokeDevice(device.id)}
                    disabled={busyId === device.id}
                    aria-label={`Remove ${device.name}`}
                  >
                    <Trash2 className="h-4 w-4 text-destructive" />
                  </Button>
                </div>
              </div>
            ))
          )}
        </div>
      </CardContent>
    </Card>
  )
}
