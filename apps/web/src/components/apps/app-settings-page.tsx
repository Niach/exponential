import { useEffect, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import type { BoardIcon } from "@exp/db-schema/domain"
import { APPS_COPY, APP_DESCRIPTION_MAX, versionLabel } from "@exp/domain-contract/apps"
import { Button, conceptIcon, FEEDBACK_WORDS, GlassGroup, Pill, Prompt, Textarea } from "@exp/ui"
import type { Team } from "@/db/schema"
import { BoardIdentityRow } from "@/components/board-form-fields"
import type { AppListEntry } from "@/hooks/use-apps"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { appCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

// VAPP-110: Settings → Apps → one app (the agent page's layout, fixture
// apps.json): the identity row (icon + colour + name, the name a deferred
// write on blur/Enter), the description card (saved on blur), Open in the
// editor, and Delete (the creator or an owner). `app` null = the create
// page: every field local until Create app, which opens the editor.

const UiDeleteIcon = conceptIcon(`ui-delete`)
const NavAppsIcon = conceptIcon(`nav-apps`)
const DEFAULT_COLOR = `#6366f1`
const DEFAULT_ICON: BoardIcon = `puzzle`

export function AppSettingsPage({
  team,
  app,
  canManage,
}: {
  team: Team
  /** The live app entry; null = the create page. */
  app: AppListEntry | null
  /** The creator or an owner may rename, describe and delete. */
  canManage: boolean
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
  const [deleting, setDeleting] = useState(false)

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
          {canManage && (
            <Button variant="destructive" size="sm" className="ml-auto" onClick={() => setDeleteOpen(true)} data-testid="app-delete">
              <UiDeleteIcon />
              {APPS_COPY.menu.delete}
            </Button>
          )}
        </div>
      )}

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
