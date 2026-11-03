import { Fragment, useEffect, useState } from "react"
import { UPSTREAM_LABELS } from "@exp/emcp/upstream"
import { Button, GlassGroup, Pill, Prompt, Switch, toast } from "@exp/ui"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpc } from "@/lib/trpc-client"
import { APP_MCP_COPY } from "@/lib/emcp/copy"
import { APPS_COPY, APPS_TEST_IDS, automationRow, webhookRow } from "@exp/domain-contract/apps"
import { promptActions, rotateWebhookSecretPrompt } from "@/lib/prompts"

// #VAPP-183: an app's MCP side on its settings page (web + desktop, the
// draft `web-editor-settings`): ACCOUNTS = the upstreams the app's tools
// call, each with the viewer's own connection and Connect; MCP TOOLS = the
// exposed tools with their Read/Write level and an owner's switch (a tool
// switched off is never listed on /api/mcp/apps). #VAPP-220: AUTOMATIONS =
// the published package's automations, each with its trigger and last-run
// state (apps.json `automationRowCases`), read-only for everyone.
// #VAPP-238: WEBHOOKS = the published package's hooks (`webhookRowCases`):
// the URL to copy, an owner's Create/Rotate secret; the minted secret is
// shown once (this component's memory only, never refetched).

type AppTools = Awaited<ReturnType<typeof trpc.appAccess.appTools.query>>

export function AppMcpSections({ appId, teamSlug, headVersion }: { appId: string; teamSlug: string; headVersion: number }) {
  const [data, setData] = useState<AppTools | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [secrets, setSecrets] = useState<Record<string, string>>({})
  const [rotateTarget, setRotateTarget] = useState<string | null>(null)

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

  if (!data || (data.tools.length === 0 && data.upstreams.length === 0 && data.automations.length === 0 && data.hooks.length === 0)) return null

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

  const copyText = (text: string) => {
    void navigator.clipboard.writeText(text).then(() => toast.success(APPS_COPY.webhooks.copied))
  }

  const mint = async (name: string) => {
    setBusy(`hook:${name}`)
    try {
      const { secret } = await trpc.appAccess.rotateHook.mutate({ appId, name }, SKIP_ERROR_TOAST)
      setSecrets((prev) => ({ ...prev, [name]: secret }))
      setData((prev) => prev && { ...prev, hooks: prev.hooks.map((h) => (h.name === name ? { ...h, hasSecret: true } : h)) })
      setRotateTarget(null)
    } catch (e) {
      failedAction(`rotate`, `the secret`, e)
    } finally {
      setBusy(null)
    }
  }

  const rotateCopy = rotateWebhookSecretPrompt(rotateTarget ?? ``)

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

      {data.automations.length > 0 && (
        <div className="space-y-1" data-testid={APPS_TEST_IDS.automations}>
          <h2 className="px-1 text-sm font-medium">{APPS_COPY.automations.title}</h2>
          <GlassGroup>
            {data.automations.map((a) => {
              const row = automationRow(a)
              return (
                <div key={a.name} className="flex items-center gap-3 px-4 py-3" data-testid={`${APPS_TEST_IDS.automationRow}-${a.name}`}>
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-mono text-sm">{a.name}</div>
                    <div className="truncate text-xs text-muted-foreground">{row.trigger}</div>
                  </div>
                  <Pill size="sm" className="shrink-0">
                    {row.label}
                  </Pill>
                </div>
              )
            })}
          </GlassGroup>
        </div>
      )}

      {data.hooks.length > 0 && (
        <div className="space-y-1" data-testid={APPS_TEST_IDS.webhooks}>
          <h2 className="px-1 text-sm font-medium">{APPS_COPY.webhooks.title}</h2>
          <GlassGroup>
            {data.hooks.map((h) => {
              const row = webhookRow(h, data.canToggle)
              const secret = secrets[h.name]
              return (
                <Fragment key={h.name}>
                  <div className="space-y-2 px-4 py-3" data-testid={`${APPS_TEST_IDS.webhookRow}-${h.name}`}>
                    <div className="flex items-center gap-3">
                      <div className="min-w-0 flex-1">
                        <div className="truncate font-mono text-sm">{h.name}</div>
                        <div className="truncate font-mono text-xs text-muted-foreground">{h.url}</div>
                      </div>
                      <Pill size="sm" className="shrink-0">
                        {row.label}
                      </Pill>
                    </div>
                    <div className="flex items-center gap-2">
                      <Button size="sm" variant="outline" onClick={() => copyText(h.url)}>
                        {APPS_COPY.webhooks.copyUrl}
                      </Button>
                      {row.action && (
                        <Button
                          size="sm"
                          variant="outline"
                          data-testid={`${APPS_TEST_IDS.webhookRow}-${h.name}-${row.action}`}
                          pending={busy === `hook:${h.name}` && rotateTarget === null}
                          disabled={busy !== null}
                          onClick={() => (row.action === `rotate` ? setRotateTarget(h.name) : void mint(h.name))}
                        >
                          {row.actionLabel}
                        </Button>
                      )}
                    </div>
                  </div>
                  {secret && (
                    <div className="flex items-center gap-3 px-4 py-3" data-testid={APPS_TEST_IDS.webhookSecret}>
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-sm">{APPS_COPY.webhooks.secret}</div>
                        <div className="truncate font-mono text-xs text-muted-foreground">{secret}</div>
                      </div>
                      <Button size="sm" variant="outline" className="shrink-0" onClick={() => copyText(secret)}>
                        {APPS_COPY.webhooks.copySecret}
                      </Button>
                    </div>
                  )}
                </Fragment>
              )
            })}
          </GlassGroup>
        </div>
      )}

      <Prompt
        open={rotateTarget !== null}
        onOpenChange={(open) => !open && busy === null && setRotateTarget(null)}
        busy={busy !== null}
        title={rotateCopy.title}
        body={rotateCopy.body}
        actions={promptActions(rotateCopy, { rotate: { busy: busy !== null, onSelect: () => void mint(rotateTarget!) } })}
      />
    </>
  )
}
