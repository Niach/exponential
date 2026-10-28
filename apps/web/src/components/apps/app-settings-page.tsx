import { useEffect, useMemo, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import type { BoardIcon } from "@exp/db-schema/domain"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { APPS_COPY, APP_DESCRIPTION_MAX, versionLabel } from "@exp/domain-contract/apps"
import { recordsCountLabel } from "@exp/domain-contract/app-records"
import { APP_ACTIONS_COPY, actionGrantable, actionTitle, declaredActions } from "@exp/domain-contract/app-actions"
import { APP_EXPORT_COPY } from "@exp/domain-contract/app-export"
import { Button, conceptIcon, FEEDBACK_WORDS, GlassGroup, Pill, Prompt, Switch, Textarea } from "@exp/ui"
import type { Team } from "@/db/schema"
import { BoardIdentityRow } from "@/components/board-form-fields"
import { AppExportDialog } from "@/components/apps/app-export-dialog"
import type { AppListEntry } from "@/hooks/use-apps"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { appCollection, appRecordCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

// VAPP-110: Settings → Apps → one app (the agent page's layout, fixture
// apps.json): the identity row (icon + colour + name, the name a deferred
// write on blur/Enter), the description card (saved on blur), Open in the
// editor, and Delete (the creator or an owner). `app` null = the create
// page: every field local until Create app, which opens the editor.

const UiDeleteIcon = conceptIcon(`ui-delete`)
const NavAppsIcon = conceptIcon(`nav-apps`)
const UiShareIcon = conceptIcon(`ui-share`)
const DEFAULT_COLOR = `#6366f1`
const DEFAULT_ICON: BoardIcon = `puzzle`

export function AppSettingsPage({
  team,
  app,
  canManage,
  canGrant = false,
}: {
  team: Team
  /** The live app entry; null = the create page. */
  app: AppListEntry | null
  /** The creator or an owner may rename, describe and delete. */
  canManage: boolean
  /** VAPP-120 (the ruling): only a team OWNER sets the app's grants; the
   *  creator gets no grant power from creating it. */
  canGrant?: boolean
}) {
  const navigate = useNavigate()
  const creating = app === null
  const readOnly = !canManage
  const [name, setName] = useState(app?.name ?? ``)
  const [icon, setIcon] = useState<BoardIcon>((app?.icon as BoardIcon | null) ?? DEFAULT_ICON)
  const [color, setColor] = useState(app?.color ?? DEFAULT_COLOR)
  const [description, setDescription] = useState(app?.description ?? ``)
  const [busy, setBusy] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [exportOpen, setExportOpen] = useState(false)
  const [deleting, setDeleting] = useState(false)
  // VAPP-122: the app's records (the synced shape) against the team's quota
  // (null = unlimited: self-hosted, or the plan still loading).
  const { data: records } = useLiveQuery(
    (query) => (app ? query.from({ r: appRecordCollection }).where(({ r }) => eq(r.appId, app.id)) : undefined),
    [app?.id]
  )
  const [recordsMax, setRecordsMax] = useState<number | null>(null)
  // VAPP-120: the typed actions the head package declares, each with its
  // grant (the synced row); a toggle writes the whole list.
  const declared = useMemo(() => declaredActions(((app?.row?.package as { functions?: string[] } | null)?.functions ?? []) as string[]), [app?.row?.package])
  const granted = (app?.row?.grantedTools ?? []) as string[]
  const [toolBusy, setToolBusy] = useState<string | null>(null)
  const toggleTool = async (name: string, on: boolean) => {
    if (!app || !canGrant || !actionGrantable(name)) return
    setToolBusy(name)
    try {
      // VAPP-158: a stored grant that is no longer grantable never rides along
      // (the server refuses it), so the toggle writes grantable names only.
      const kept = granted.filter((g) => g !== name && actionGrantable(g))
      const next = on ? [...kept, name] : kept
      const result = await trpc.apps.update.mutate({ id: app.id, grantedTools: next }, SKIP_ERROR_TOAST)
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
      .query({ teamId: team.id })
      .then((plan) => {
        if (live) setRecordsMax(Number.isFinite(plan.limits.appRecords) ? plan.limits.appRecords : null)
      })
      .catch(() => undefined)
    return () => {
      live = false
    }
  }, [app, team.id])

  // Seed when the APP changes, never on a resync of the same one.
  const appId = app?.id ?? null
  useEffect(() => {
    setName(app?.name ?? ``)
    setIcon((app?.icon as BoardIcon | null) ?? DEFAULT_ICON)
    setColor(app?.color ?? DEFAULT_COLOR)
    setDescription(app?.description ?? ``)
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

  const create = async () => {
    if (busy || !name.trim()) return
    setBusy(true)
    try {
      const result = await trpc.apps.create.mutate(
        { teamId: team.id, name: name.trim(), icon, color, description: description.trim() || undefined },
        SKIP_ERROR_TOAST
      )
      await appCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
      void navigate({ to: `/t/$teamSlug/apps/$appSlug`, params: { teamSlug: team.slug, appSlug: result.app.slug } })
    } catch (error) {
      failedAction(`create`, `the app`, error)
    } finally {
      setBusy(false)
    }
  }

  const confirmDelete = async () => {
    if (!app) return
    setDeleting(true)
    try {
      await trpc.apps.delete.mutate({ id: app.id }, SKIP_ERROR_TOAST)
      setDeleteOpen(false)
      void navigate({ to: `/t/$teamSlug/settings/apps`, params: { teamSlug: team.slug }, replace: true })
    } catch (error) {
      failedAction(`delete`, `the app`, error)
    } finally {
      setDeleting(false)
    }
  }

  return (
    <div className="space-y-4" data-testid="app-settings-page">
      <GlassGroup>
        <BoardIdentityRow
          name={name}
          onNameChange={setName}
          onNameBlur={creating ? undefined : saveName}
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
          autoFocus={creating}
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

      {!creating && (
        <p className="px-1 text-sm text-muted-foreground" data-testid="app-settings-records">
          {recordsCountLabel(records?.length ?? 0, recordsMax)}
        </p>
      )}

      {!creating && (
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
      )}

      {creating ? (
        <div className="flex justify-end pt-2">
          <Button onClick={() => void create()} disabled={!name.trim()} pending={busy} pendingLabel={FEEDBACK_WORDS.pending.save} data-testid="app-create">
            {APPS_COPY.create}
          </Button>
        </div>
      ) : (
        <div className="flex flex-wrap items-center gap-2 pt-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => void navigate({ to: `/t/$teamSlug/apps/$appSlug`, params: { teamSlug: team.slug, appSlug: app.slug } })}
            data-testid="app-open-editor"
          >
            <NavAppsIcon />
            {APPS_COPY.openEditor}
          </Button>
          <Pill size="sm" className="tabular-nums">{versionLabel(app.headVersion)}</Pill>
          {app.headVersion > 0 && (
            <Button variant="outline" size="sm" onClick={() => setExportOpen(true)} data-testid="app-export">
              <UiShareIcon />
              {APP_EXPORT_COPY.export}
            </Button>
          )}
          {canManage && (
            <Button variant="destructive" size="sm" className="ml-auto" onClick={() => setDeleteOpen(true)} data-testid="app-delete">
              <UiDeleteIcon />
              {APPS_COPY.menu.delete}
            </Button>
          )}
        </div>
      )}

      {!creating && <AppExportDialog app={exportOpen ? app : null} onOpenChange={setExportOpen} />}
      <Prompt
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        busy={deleting}
        title={APPS_COPY.deleteTitle.replace(`{name}`, app?.name ?? ``)}
        body={APPS_COPY.deleteBody}
        actions={[
          { label: `Cancel`, role: `cancel` },
          { label: APPS_COPY.delete, role: `destructive`, busy: deleting, onSelect: confirmDelete, testId: `app-delete-confirm` },
        ]}
      />
    </div>
  )
}
