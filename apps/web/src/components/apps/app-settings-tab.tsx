import { useEffect, useMemo, useState } from "react"
import type { BoardIcon } from "@exp/db-schema/domain"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { APPS_COPY, APP_DESCRIPTION_MAX } from "@exp/domain-contract/apps"
import { recordsCountLabel } from "@exp/domain-contract/app-records"
import { APP_ACTIONS_COPY, actionGrantable, actionTitle, declaredActions } from "@exp/domain-contract/app-actions"
import { Button, conceptIcon, GlassGroup, Switch, Textarea } from "@exp/ui"
import { BoardIdentityRow } from "@/components/board-form-fields"
import type { AppListEntry } from "@/hooks/use-apps"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { appCollection, appRecordCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { grantableTools } from "@/lib/app-grants"
import { AppMcpSections } from "@/components/apps/app-mcp-sections"

// VAPP-110 → VAPP-177 (fixtures/apps.json THE SETTINGS TAB): the editor's
// Settings tab (web ≥ md) and the phone menu's Settings sheet: the identity
// row (icon + colour + name, the name a deferred write on blur/Enter), the
// description card (saved on blur), the records line, the Tools group (the
// grant switches, an owner only), then Export (head > 0) and Delete app (the
// creator or an owner). The editor owns the export sheet and the delete
// prompt (the phone menu opens the same ones). Everyone else reads it.

const UiDeleteIcon = conceptIcon(`ui-delete`)
const UiShareIcon = conceptIcon(`ui-share`)
const DEFAULT_COLOR = `#6366f1`
const DEFAULT_ICON: BoardIcon = `puzzle`

export function AppSettingsTab({
  teamId,
  teamSlug,
  app,
  canManage,
  canGrant = false,
  onExport,
  onDelete,
}: {
  teamId: string
  teamSlug: string
  /** The live app entry. */
  app: AppListEntry
  /** The creator or an owner may rename, describe and delete. */
  canManage: boolean
  /** VAPP-120 (the ruling): only a team OWNER sets the app's grants; the
   *  creator gets no grant power from creating it. */
  canGrant?: boolean
  /** Opens the editor's export sheet. */
  onExport: () => void
  /** Opens the editor's delete prompt. */
  onDelete: () => void
}) {
  const readOnly = !canManage
  const [name, setName] = useState(app.name ?? ``)
  const [icon, setIcon] = useState<BoardIcon>((app.icon as BoardIcon | null) ?? DEFAULT_ICON)
  const [color, setColor] = useState(app.color ?? DEFAULT_COLOR)
  const [description, setDescription] = useState(app.description ?? ``)
  // VAPP-122: the app's records (the synced shape) against the team's quota
  // (null = unlimited: self-hosted, or the plan still loading).
  const { data: records } = useLiveQuery(
    (query) => (app ? query.from({ r: appRecordCollection }).where(({ r }) => eq(r.appId, app.id)) : undefined),
    [app.id]
  )
  const [recordsMax, setRecordsMax] = useState<number | null>(null)
  // VAPP-120: the typed actions the head package declares, each with its
  // grant (the synced row); a toggle writes the whole list.
  const declared = useMemo(() => declaredActions(((app.row?.package as { functions?: string[] } | null)?.functions ?? []) as string[]), [app.row?.package])
  // FEED-159: a stored grant that is no longer grantable reads as none.
  const granted = grantableTools(app.row?.grantedTools as string[] | null | undefined)
  const [toolBusy, setToolBusy] = useState<string | null>(null)
  const toggleTool = async (name: string, on: boolean) => {
    if (!app || !canGrant || !actionGrantable(name)) return
    setToolBusy(name)
    try {
      // VAPP-158: a stored grant that is no longer grantable never rides along
      // (the server refuses it), so the toggle writes grantable names only.
      // VAPP-156: the grant names the head version on screen; a newer one
      // (published meanwhile) refuses, the toast says so and the page resyncs.
      const kept = granted.filter((g) => g !== name)
      const next = on ? [...kept, name] : kept
      const result = await trpc.apps.update.mutate({ id: app.id, headVersion: app.headVersion, grantedTools: next }, SKIP_ERROR_TOAST)
      await appCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
    } catch (error) {
      failedAction(`save`, `the app`, error)
    } finally {
      setToolBusy(null)
    }
  }
  useEffect(() => {
    if (!app) return
    let live = true
    void trpc.billing.teamPlan
      .query({ teamId: teamId })
      .then((plan) => {
        if (live) setRecordsMax(Number.isFinite(plan.limits.appRecords) ? plan.limits.appRecords : null)
      })
      .catch(() => undefined)
    return () => {
      live = false
    }
  }, [app, teamId])

  // Seed when the APP changes, never on a resync of the same one.
  const appId = app.id ?? null
  useEffect(() => {
    setName(app.name ?? ``)
    setIcon((app.icon as BoardIcon | null) ?? DEFAULT_ICON)
    setColor(app.color ?? DEFAULT_COLOR)
    setDescription(app.description ?? ``)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [appId])

  const patch = async (values: { name?: string; icon?: BoardIcon; color?: string; description?: string }) => {
    if (!app || readOnly) return
    try {
      const result = await trpc.apps.update.mutate({ id: app.id, ...values }, SKIP_ERROR_TOAST)
      await appCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
    } catch (error) {
      failedAction(`save`, `the app`, error)
    }
  }

  const saveName = () => {
    const trimmed = name.trim()
    if (!app || !trimmed || trimmed === app.name) {
      if (app && !trimmed) setName(app.name)
      return
    }
    void patch({ name: trimmed })
  }

  return (
    <div className="space-y-4">
      <GlassGroup>
        <BoardIdentityRow
          name={name}
          onNameChange={setName}
          onNameBlur={saveName}
          placeholder={APPS_COPY.namePlaceholder}
          icon={icon}
          onIconChange={(next) => {
            setIcon(next)
            void patch({ icon: next })
          }}
          color={color}
          onColorChange={(next) => {
            setColor(next)
            void patch({ color: next })
          }}
          disabled={readOnly}
        />
      </GlassGroup>

      <div className="space-y-1">
        <Textarea
          value={description}
          onChange={(event) => setDescription(event.target.value.slice(0, APP_DESCRIPTION_MAX))}
          onBlur={() => {
            if (app && description !== app.description) void patch({ description })
          }}
          placeholder={APPS_COPY.descriptionPlaceholder}
          rows={3}
          disabled={readOnly}
          data-testid="app-settings-description"
        />
      </div>

      <p className="px-1 text-sm text-muted-foreground" data-testid="app-settings-records">
        {recordsCountLabel(records?.length ?? 0, recordsMax)}
      </p>

      <div className="space-y-1" data-testid="app-settings-tools">
          <div className="px-1">
            <h2 className="text-sm font-medium">{APP_ACTIONS_COPY.title}</h2>
            <p className="text-sm text-muted-foreground">{APP_ACTIONS_COPY.hint}</p>
          </div>
          <GlassGroup>
            {declared.length === 0 ? (
              <p className="px-4 py-3 text-sm text-muted-foreground">{APP_ACTIONS_COPY.none}</p>
            ) : (
              declared.map((name) => (
                <div key={name} className="flex items-center justify-between gap-3 px-4 py-3" data-testid={`app-tool-${name}`}>
                  <div className="min-w-0">
                    <div className="truncate text-sm">{actionTitle(name)}</div>
                    <div className="truncate font-mono text-xs text-muted-foreground">{name}</div>
                  </div>
                  {actionGrantable(name) ? (
                    <Switch
                      checked={granted.includes(name)}
                      disabled={!canGrant || toolBusy === name}
                      onCheckedChange={(next) => void toggleTool(name, next)}
                      aria-label={`${APP_ACTIONS_COPY.granted}: ${actionTitle(name)}`}
                    />
                  ) : (
                    // The ruling: a destructive or owner-only action is never grantable.
                    <span className="shrink-0 text-sm text-muted-foreground">{APP_ACTIONS_COPY.always}</span>
                  )}
                </div>
              ))
            )}
          </GlassGroup>
        </div>

        <AppMcpSections appId={app.id} teamSlug={teamSlug} headVersion={app.headVersion} />

        {(app.headVersion > 0 || canManage) && (
        <div className="flex flex-wrap items-center gap-2 pt-2">
          {app.headVersion > 0 && (
            <Button variant="outline" size="sm" onClick={onExport} data-testid="app-export">
              <UiShareIcon />
              {APPS_COPY.menu.export}
            </Button>
          )}
          {canManage && (
            <Button variant="destructive" size="sm" className="ml-auto" onClick={onDelete} data-testid="app-delete">
              <UiDeleteIcon />
              {APPS_COPY.menu.delete}
            </Button>
          )}
        </div>
        )}
    </div>
  )
}
