import { useEffect, useRef, useState } from "react"
import { missingUpstreams, UPSTREAM_LABELS, type UpstreamState } from "@exp/emcp/upstream"
import {
  BoardGlyph,
  Button,
  Checkbox,
  EmptyState,
  FEEDBACK_ERRORS,
  FEEDBACK_WORDS,
  Label,
  SegmentedControl,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  SkeletonRows,
  Switch,
  conceptIcon,
} from "@exp/ui"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpc } from "@/lib/trpc-client"
import type { ConsentApp, ConsentTeam } from "@/lib/emcp/apps-consent"

// #VAPP-182: the consent of the apps endpoint (`/api/mcp/apps`, reached as
// `/auth/consent?resource=apps`): ONE team, all its apps (including later
// ones) or a list, Read only | Full, and Connect for an upstream an app needs
// that the member has not connected. Connect runs the server's own sign-in
// and comes back here (`returnTo` = this page), so the client sees one flow;
// the choice survives the round trip in sessionStorage.

type Access = `read` | `full`

interface Choice {
  teamId: string | null
  allApps: boolean
  appIds: string[]
  access: Access
}

const AppsIcon = conceptIcon(`nav-apps`)
const ACCESS_OPTIONS = [
  { value: `read` as const, label: `Read only` },
  { value: `full` as const, label: `Full` },
]

const storageKey = (consentCode: string) => `exp:apps-consent:${consentCode}`

function savedChoice(consentCode: string): Choice | null {
  try {
    const raw = sessionStorage.getItem(storageKey(consentCode))
    return raw ? (JSON.parse(raw) as Choice) : null
  } catch {
    return null
  }
}

/** The upstreams an app's tools need, with this member's state of each. */
function upstreamStates(app: ConsentApp): Record<string, UpstreamState> {
  return Object.fromEntries(app.upstreams.filter((u) => u.state).map((u) => [u.name, u.state!]))
}

function appCaption(app: ConsentApp): string {
  const tools = `${app.toolCount} ${app.toolCount === 1 ? `tool` : `tools`}`
  const needs = app.upstreams.map((u) => u.name)
  return needs.length ? `${tools} · needs ${needs.join(`, `)}` : tools
}

export function AppsConsent({
  clientId,
  clientName,
  consentCode,
}: {
  clientId: string
  clientName: string | null
  consentCode: string
}) {
  const [tree, setTree] = useState<ConsentTeam[] | null>(null)
  const [loadFailed, setLoadFailed] = useState(false)
  const [loadAttempt, setLoadAttempt] = useState(0)
  const retried = useRef(false)
  const retryLoad = () => {
    retried.current = true
    setLoadFailed(false)
    setLoadAttempt((n) => n + 1)
  }
  const [choice, setChoice] = useState<Choice>(
    () => savedChoice(consentCode) ?? { teamId: null, allApps: true, appIds: [], access: `full` }
  )
  const [pending, setPending] = useState<`allow` | `deny` | string | null>(null)

  useEffect(() => {
    let cancelled = false
    trpc.mcpGrants.appsTree
      .query()
      .then(({ teams }) => {
        if (cancelled) return
        setTree(teams)
        setChoice((c) => (c.teamId && teams.some((t) => t.id === c.teamId) ? c : { ...c, teamId: teams[0]?.id ?? null }))
      })
      .catch((e: unknown) => {
        if (cancelled) return
        setLoadFailed(true)
        const tapped = retried.current
        retried.current = false
        if (tapped) failedAction(`load`, `the consent request`, e, { retry: retryLoad })
      })
    return () => {
      cancelled = true
    }
  }, [loadAttempt])

  const team = tree?.find((t) => t.id === choice.teamId) ?? null
  const chosenApps = team ? (choice.allApps ? team.apps : team.apps.filter((a) => choice.appIds.includes(a.id))) : []
  const canAllow = team !== null && team.apps.length > 0 && chosenApps.length > 0
  // #VAPP-183: the member's policy caps the grant: when no chosen app allows
  // writes, Full has nothing to add.
  const readCapped = chosenApps.length > 0 && chosenApps.every((a) => a.level !== `write`)
  const access: Access = readCapped ? `read` : choice.access

  const connect = async (serverId: string, auth: string | null) => {
    if (auth !== `oauth`) {
      window.open(`/t/${team?.slug}/settings/mcp-servers?connect=${serverId}`, `_blank`, `noopener`)
      return
    }
    setPending(serverId)
    sessionStorage.setItem(storageKey(consentCode), JSON.stringify(choice))
    try {
      const here = `${window.location.pathname}${window.location.search}`
      const { authorizeUrl } = await trpc.mcpServers.connect.mutate({ serverId, returnTo: here }, SKIP_ERROR_TOAST)
      window.location.href = authorizeUrl
    } catch (e) {
      failedAction(`connect`, `the account`, e)
      setPending(null)
    }
  }

  const respond = async (accept: boolean) => {
    setPending(accept ? `allow` : `deny`)
    try {
      const { redirectURI } = await trpc.mcpGrants.grantAppsAndConsent.mutate(
        {
          clientId,
          consentCode,
          accept,
          teamId: choice.teamId ?? undefined,
          allApps: choice.allApps,
          appIds: choice.allApps ? [] : choice.appIds,
          access,
        },
        SKIP_ERROR_TOAST
      )
      sessionStorage.removeItem(storageKey(consentCode))
      window.location.href = redirectURI
    } catch (e) {
      failedAction(accept ? `allow` : `deny`, `access`, e)
      setPending(null)
    }
  }

  if (!tree && !loadFailed) return <SkeletonRows rowHeight={32} className="rounded-md border" />

  return (
    <div className="space-y-4" data-testid="apps-consent">
      {tree && tree.length > 1 && (
        <Select value={choice.teamId ?? undefined} onValueChange={(teamId) => setChoice({ ...choice, teamId, appIds: [] })}>
          <SelectTrigger className="w-full" aria-label="Team">
            <SelectValue placeholder="Team" />
          </SelectTrigger>
          <SelectContent>
            {tree.map((t) => (
              <SelectItem key={t.id} value={t.id}>
                {t.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      )}

      {team && team.apps.length > 0 && (
        <div className="flex items-start justify-between gap-3 rounded-md border p-3">
          <div className="space-y-0.5">
            <Label htmlFor="consent-all-apps">All apps</Label>
            <p className="text-xs text-muted-foreground">Every app of {team.name}, including ones created later.</p>
          </div>
          <Switch
            id="consent-all-apps"
            checked={choice.allApps}
            onCheckedChange={(checked) => setChoice({ ...choice, allApps: checked === true })}
          />
        </div>
      )}

      <div className="max-h-80 divide-y overflow-y-auto rounded-md border">
        {(!team || team.apps.length === 0) && <EmptyState size="list" icon={AppsIcon} title="No apps with tools yet" />}
        {team?.apps.map((app) => {
          const checked = choice.allApps || choice.appIds.includes(app.id)
          const missing = new Set(
            missingUpstreams([app.upstreams.map((u) => u.name)], upstreamStates(app), app.upstreams.filter((u) => u.serverId).map((u) => u.name))
          )
          return (
            <div key={app.id} className="flex items-center gap-3 px-3 py-2.5" data-app={app.slug}>
              <Checkbox
                id={`consent-app-${app.id}`}
                disabled={choice.allApps}
                checked={checked}
                onCheckedChange={(on) =>
                  setChoice({
                    ...choice,
                    appIds: on === true ? [...new Set([...choice.appIds, app.id])] : choice.appIds.filter((id) => id !== app.id),
                  })
                }
              />
              <BoardGlyph board={{ icon: app.icon, color: app.color }} />
              <Label htmlFor={`consent-app-${app.id}`} className="min-w-0 flex-1 flex-col items-start gap-0 font-normal">
                <span className="truncate font-medium">{app.name}</span>
                <span className="truncate text-xs text-muted-foreground">{appCaption(app)}</span>
              </Label>
              <div className="flex shrink-0 items-center gap-2">
                {app.upstreams.map((u) =>
                  u.serverId && missing.has(u.name) ? (
                    <Button
                      key={u.name}
                      type="button"
                      size="sm"
                      variant="outline"
                      pending={pending === u.serverId}
                      disabled={pending !== null}
                      onClick={() => void connect(u.serverId!, u.auth)}
                    >
                      {app.upstreams.length > 1 ? `Connect ${u.name}` : `Connect`}
                    </Button>
                  ) : u.state ? (
                    <span key={u.name} className="text-xs text-muted-foreground">
                      {UPSTREAM_LABELS[u.state]}
                    </span>
                  ) : null
                )}
              </div>
            </div>
          )
        })}
      </div>

      {loadFailed && (
        <Button type="button" variant="outline" size="sm" onClick={retryLoad}>
          {FEEDBACK_ERRORS.retry}
        </Button>
      )}

      <SegmentedControl<Access>
        value={access}
        onValueChange={(next) => setChoice({ ...choice, access: next })}
        options={ACCESS_OPTIONS.map((o) => (o.value === `full` && readCapped ? { ...o, disabled: true } : o))}
      />

      <p className="text-xs text-muted-foreground">
        {clientName ?? `The client`} runs the chosen apps' tools as you.
        {access === `read` ? ` Read only hides every tool that writes.` : ``}
      </p>

      <div className="flex gap-2">
        <Button
          type="button"
          variant="outline"
          className="flex-1"
          pending={pending === `deny`}
          disabled={pending !== null}
          onClick={() => void respond(false)}
        >
          Deny
        </Button>
        <Button
          type="button"
          className="flex-1"
          pending={pending === `allow`}
          pendingLabel={FEEDBACK_WORDS.pending.connect}
          disabled={pending !== null || !canAllow}
          onClick={() => void respond(true)}
        >
          Allow access
        </Button>
      </div>
    </div>
  )
}
