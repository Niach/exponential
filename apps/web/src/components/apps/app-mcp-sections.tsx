import { useEffect, useState } from "react"
import { UPSTREAM_LABELS } from "@exp/emcp/upstream"
import { Button, GlassGroup, Pill, Switch } from "@exp/ui"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpc } from "@/lib/trpc-client"
import { APP_MCP_COPY } from "@/lib/emcp/copy"

// #VAPP-183: an app's MCP side on its settings page (web + desktop, the
// draft `web-editor-settings`): ACCOUNTS = the upstreams the app's tools
// call, each with the viewer's own connection and Connect; MCP TOOLS = the
// exposed tools with their Read/Write level and an owner's switch (a tool
// switched off is never listed on /api/mcp/apps).

type AppTools = Awaited<ReturnType<typeof trpc.appAccess.appTools.query>>

export function AppMcpSections({ appId, teamSlug, headVersion }: { appId: string; teamSlug: string; headVersion: number }) {
  const [data, setData] = useState<AppTools | null>(null)
  const [busy, setBusy] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    trpc.appAccess.appTools
      .query({ appId })
      .then((next) => !cancelled && setData(next))
      .catch(() => !cancelled && setData(null))
    return () => {
      cancelled = true
    }
  }, [appId, headVersion])

  if (!data || (data.tools.length === 0 && data.upstreams.length === 0)) return null

  const connect = async (serverId: string, auth: string | null) => {
    if (auth !== `oauth`) {
      window.location.href = `/t/${teamSlug}/settings/mcp?connect=${serverId}`
      return
    }
    setBusy(serverId)
    try {
      const { authorizeUrl } = await trpc.mcpServers.connect.mutate({ serverId, returnTo: window.location.pathname }, SKIP_ERROR_TOAST)
      window.location.href = authorizeUrl
    } catch (e) {
      failedAction(`connect`, `the account`, e)
      setBusy(null)
    }
  }

  const toggle = async (tool: string, enabled: boolean) => {
    setBusy(tool)
    const before = data
    setData({ ...data, tools: data.tools.map((t) => (t.name === tool ? { ...t, enabled } : t)) })
    try {
      await trpc.appAccess.setToolEnabled.mutate({ appId, tool, enabled }, SKIP_ERROR_TOAST)
    } catch (e) {
      setData(before)
      failedAction(`save`, `the tool`, e)
    } finally {
      setBusy(null)
    }
  }

  return (
    <>
      {data.upstreams.length > 0 && (
        <div className="space-y-1" data-testid="app-settings-accounts">
          <h2 className="px-1 text-sm font-medium">{APP_MCP_COPY.accounts}</h2>
          <GlassGroup>
            {data.upstreams.map((u) => (
              <div key={u.name} className="flex items-center justify-between gap-3 px-4 py-3" data-testid={`app-account-${u.name}`}>
                <div className="min-w-0 truncate text-sm">{u.name}</div>
                {!u.serverId ? (
                  <span className="shrink-0 text-sm text-muted-foreground">{APP_MCP_COPY.noServer}</span>
                ) : u.state === `connected` ? (
                  <span className="shrink-0 text-sm text-muted-foreground">{UPSTREAM_LABELS.connected}</span>
                ) : (
                  <Button size="sm" variant="outline" pending={busy === u.serverId} disabled={busy !== null} onClick={() => void connect(u.serverId!, u.auth)}>
                    {u.state === `reconnect` ? APP_MCP_COPY.reconnect : APP_MCP_COPY.connect}
                  </Button>
                )}
              </div>
            ))}
          </GlassGroup>
        </div>
      )}

      {data.tools.length > 0 && (
        <div className="space-y-1" data-testid="app-settings-mcp-tools">
          <h2 className="px-1 text-sm font-medium">{APP_MCP_COPY.tools}</h2>
          <GlassGroup>
            {data.tools.map((t) => (
              <div key={t.name} className="flex items-center gap-3 px-4 py-3" data-testid={`app-mcp-tool-${t.name}`}>
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm">{t.title}</div>
                  <div className="truncate font-mono text-xs text-muted-foreground">{t.name}</div>
                </div>
                <Pill size="sm" className="shrink-0">
                  {t.access === `read` ? APP_MCP_COPY.read : APP_MCP_COPY.write}
                </Pill>
                <Switch
                  checked={t.enabled}
                  disabled={!data.canToggle || busy === t.name}
                  onCheckedChange={(next) => void toggle(t.name, next === true)}
                  aria-label={`${APP_MCP_COPY.tools}: ${t.title}`}
                />
              </div>
            ))}
          </GlassGroup>
        </div>
      )}
    </>
  )
}
