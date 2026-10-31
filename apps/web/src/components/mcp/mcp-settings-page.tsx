import { useCallback, useEffect, useState } from "react"
import { Link } from "@tanstack/react-router"
import {
  SETTINGS_MCP,
  clientCaption,
  levelLabel,
  toolCountLabel,
  type McpPolicyLevel,
  type McpScope,
} from "@exp/domain-contract/settings-mcp"
import {
  BoardGlyph,
  Button,
  GlassSectionHeader,
  ListRow,
  Picker,
  Prompt,
  SegmentedControl,
  SETTINGS_LIST_CLASS,
  SkeletonRows,
  UserAvatar,
  conceptIcon,
  toast,
} from "@exp/ui"
import { BuiltinToolsGroup, TeamMcpServersSection, type McpSettingsRequest } from "@/components/team/mcp-servers-section"
import { Empty } from "@/components/empty"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { compactRelativeTime } from "@/lib/relative-time"
import { promptActions, revokeMcpClientPrompt } from "@/lib/prompts"
import { trpc } from "@/lib/trpc-client"

// #VAPP-185 (fixtures/settings-mcp.json): Settings › MCP, ONE page ×4:
// Endpoint · Servers · Clients (Mine | Everyone) · Access (+ Activity) ·
// Built-in Exponential tools. It replaces the MCP servers page.

const McpIcon = conceptIcon(`settings-mcp`)
const ChevronIcon = conceptIcon(`ui-chevron-right`)
const S = SETTINGS_MCP

type Clients = Awaited<ReturnType<typeof trpc.mcpClients.list.query>>
type Access = Awaited<ReturnType<typeof trpc.appAccess.list.query>>

export function McpSettingsPage({
  teamId,
  teamSlug,
  isOwner,
  request,
  onRequestConsumed,
}: {
  teamId: string
  teamSlug: string
  isOwner: boolean
  request?: McpSettingsRequest
  onRequestConsumed?: () => void
}) {
  return (
    <div data-testid={S.testIds.page}>
      <EndpointSection />
      <div data-testid={S.testIds.servers}>
        <TeamMcpServersSection teamId={teamId} isOwner={isOwner} request={request} onRequestConsumed={onRequestConsumed} />
      </div>
      <ClientsSection teamId={teamId} isOwner={isOwner} />
      <AccessSection teamId={teamId} teamSlug={teamSlug} />
      <div data-testid={S.testIds.builtin}>
        <BuiltinToolsGroup />
      </div>
    </div>
  )
}

function EndpointSection() {
  const url = `${window.location.origin}${S.endpointPath}`
  return (
    <div className="mb-6" data-testid={S.testIds.endpoint}>
      <GlassSectionHeader label={S.labels.endpoint} />
      <div className={SETTINGS_LIST_CLASS}>
        <ListRow className="gap-3 px-3 py-2">
          <span className="min-w-0 flex-1 truncate font-mono text-sm">{url}</span>
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              void navigator.clipboard.writeText(url).then(() => toast.success(S.copied))
            }}
          >
            {S.copy}
          </Button>
        </ListRow>
      </div>
    </div>
  )
}

function ClientsSection({ teamId, isOwner }: { teamId: string; isOwner: boolean }) {
  const [scope, setScope] = useState<McpScope>(`mine`)
  const [data, setData] = useState<Clients | null>(null)
  const [target, setTarget] = useState<Clients[`clients`][number] | null>(null)
  const [busy, setBusy] = useState(false)
  const load = useCallback(() => {
    trpc.mcpClients.list
      .query({ teamId, scope })
      .then(setData)
      .catch(() => setData({ canSeeEveryone: false, clients: [] }))
  }, [teamId, scope])
  useEffect(load, [load])

  const revoke = async () => {
    if (!target) return
    setBusy(true)
    try {
      await trpc.mcpClients.revoke.mutate(
        { clientId: target.clientId, teamId, userId: scope === `everyone` ? target.userId : undefined },
        SKIP_ERROR_TOAST
      )
      setTarget(null)
      load()
    } catch (e) {
      failedAction(`revoke`, `the client`, e)
    } finally {
      setBusy(false)
    }
  }

  const revokeCopy = revokeMcpClientPrompt(target?.name ?? `this client`)

  return (
    <div className="mb-6" data-testid={S.testIds.clients}>
      <GlassSectionHeader label={S.labels.clients} />
      {isOwner && (
        <SegmentedControl<McpScope>
          className="mb-2"
          value={scope}
          onValueChange={setScope}
          options={S.scopes.map((s) => ({ value: s.value as McpScope, label: s.label }))}
        />
      )}
      {data === null ? (
        <SkeletonRows rowHeight={52} />
      ) : data.clients.length === 0 ? (
        <Empty id="mcp-clients" />
      ) : (
        <div className={SETTINGS_LIST_CLASS}>
          {data.clients.map((c) => (
            <ListRow key={`${c.clientId}:${c.userId}`} className="gap-3 px-3 py-2" data-testid="mcp-client-row">
              {c.icon ? <img src={c.icon} alt="" className="size-4 shrink-0 rounded-sm" /> : <McpIcon className="size-4 shrink-0 text-foreground/70" />}
              <div className="min-w-0 flex-1">
                <div className="truncate text-sm font-medium">{c.name}</div>
                <div className="truncate text-xs text-muted-foreground">
                  {clientCaption(scope, { ...c, lastUsed: c.lastUsedAt ? compactRelativeTime(c.lastUsedAt) : null })}
                </div>
              </div>
              <Button size="sm" variant="outline" onClick={() => setTarget(c)}>
                {S.revoke}
              </Button>
            </ListRow>
          ))}
        </div>
      )}
      <Prompt
        open={target !== null}
        onOpenChange={(open) => !open && setTarget(null)}
        busy={busy}
        title={revokeCopy.title}
        body={revokeCopy.body}
        actions={promptActions(revokeCopy, { revoke: { busy, onSelect: () => void revoke() } })}
      />
    </div>
  )
}

function AccessSection({ teamId, teamSlug }: { teamId: string; teamSlug: string }) {
  const [data, setData] = useState<Access | null>(null)
  useEffect(() => {
    trpc.appAccess.list
      .query({ teamId })
      .then(setData)
      .catch(() => setData(null))
  }, [teamId])
  if (!data || data.apps.length === 0) return null

  const setLevel = async (appId: string, userId: string, level: McpPolicyLevel) => {
    const before = data
    setData({ ...data, rows: data.rows.map((r) => (r.userId === userId ? { ...r, levels: { ...r.levels, [appId]: level } } : r)) })
    try {
      await trpc.appAccess.set.mutate({ appId, userId, level }, SKIP_ERROR_TOAST)
    } catch (e) {
      setData(before)
      failedAction(`save`, `the access`, e)
    }
  }

  return (
    <div className="mb-6" data-testid={S.testIds.access}>
      <GlassSectionHeader label={S.labels.access} />
      <div className="space-y-3">
        {data.apps.map((app) => (
          <div key={app.id} className={SETTINGS_LIST_CLASS} data-app={app.slug}>
            <ListRow className="gap-3 px-3 py-2">
              <BoardGlyph board={{ icon: app.icon, color: app.color }} />
              <span className="min-w-0 flex-1 truncate text-sm font-medium">{app.name}</span>
              <span className="shrink-0 text-xs text-muted-foreground">{toolCountLabel(app.toolCount)}</span>
            </ListRow>
            {data.rows.map((m) => (
              <ListRow key={m.userId} className="gap-3 px-3 py-2">
                <UserAvatar user={{ id: m.userId, name: m.name, image: m.image }} size={20} />
                <span className="min-w-0 flex-1 truncate text-sm">{m.name}</span>
                {data.canEdit ? (
                  <Picker<McpPolicyLevel>
                    mode="single"
                    triggerVariant="inline"
                    mobileTitle={m.name}
                    value={m.levels[app.id] ?? `write`}
                    onChange={(level) => void setLevel(app.id, m.userId, level)}
                    items={S.levels.map((l) => ({ value: l.value as McpPolicyLevel, label: l.label }))}
                    data-testid={`mcp-access-${app.slug}-${m.userId}`}
                  />
                ) : (
                  <span className="shrink-0 text-sm text-muted-foreground">{levelLabel(m.levels[app.id] ?? `write`)}</span>
                )}
              </ListRow>
            ))}
          </div>
        ))}
        <div className={SETTINGS_LIST_CLASS}>
          <Link to="/t/$teamSlug/settings/mcp/activity" params={{ teamSlug }} data-testid={S.testIds.activity}>
            <ListRow className="gap-3 px-3 py-2">
              <span className="min-w-0 flex-1 text-sm">{S.activity}</span>
              <ChevronIcon className="size-4 shrink-0 text-muted-foreground" />
            </ListRow>
          </Link>
        </div>
      </div>
    </div>
  )
}
