import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { APPS_COPY, APPS_TEST_IDS, versionLabel } from "@exp/domain-contract/apps"
import { APP_EXPORT_COPY } from "@exp/domain-contract/app-export"
import {
  BoardGlyph,
  Button,
  conceptIcon,
  EmptyState,
  GlassSectionHeader,
  ListRow,
  Menu,
  Pill,
  Prompt,
  SETTINGS_LIST_CLASS,
} from "@exp/ui"
import { AppExportDialog } from "@/components/apps/app-export-dialog"
import { AppFormDialog } from "@/components/apps/app-form-dialog"
import type { AppListEntry } from "@/hooks/use-apps"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { appCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

// VAPP-110 (fixtures/apps.json): the Apps list = the band with `New app` as
// its trailing pill, then one ListRow per app (glyph, name, the version
// pill, the `…` menu: Rename, Delete). Manage = create, rename, delete, open.

const NavAppsIcon = conceptIcon(`nav-apps`)
const UiAddIcon = conceptIcon(`ui-add`)
const UiMoreIcon = conceptIcon(`ui-more`)
const UiEditIcon = conceptIcon(`ui-edit`)
const UiShareIcon = conceptIcon(`ui-share`)
const UiDeleteIcon = conceptIcon(`ui-delete`)

export function AppsList({
  teamId,
  teamSlug,
  apps,
  me,
  isOwner,
  rowsOpen = `editor`,
}: {
  teamId: string
  teamSlug: string
  apps: readonly AppListEntry[]
  me: string | null
  isOwner: boolean
  /** Where a row goes: the editor (the Apps page) or the app's settings page (Settings → Apps). */
  rowsOpen?: `editor` | `settings`
}) {
  const navigate = useNavigate()
  const [creating, setCreating] = useState(false)
  const [renaming, setRenaming] = useState<AppListEntry | null>(null)
  const [exporting, setExporting] = useState<AppListEntry | null>(null)
  const [deleting, setDeleting] = useState<AppListEntry | null>(null)
  const [busy, setBusy] = useState(false)

  const open = (app: AppListEntry) =>
    void (rowsOpen === `settings`
      ? navigate({ to: `/t/$teamSlug/settings/apps/$appId`, params: { teamSlug, appId: app.id } })
      : navigate({ to: `/t/$teamSlug/apps/$appSlug`, params: { teamSlug, appSlug: app.slug } }))

  const remove = async () => {
    if (!deleting) return
    setBusy(true)
    try {
      const result = await trpc.apps.delete.mutate({ id: deleting.id }, SKIP_ERROR_TOAST)
      await appCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
      setDeleting(null)
    } catch (error) {
      failedAction(`delete`, `the app`, error)
    } finally {
      setBusy(false)
    }
  }

  const canManage = (app: AppListEntry) => isOwner || app.createdBy === me

  return (
    <div data-testid={APPS_TEST_IDS.list}>
      <GlassSectionHeader
        label={APPS_COPY.title}
        trailing={
          <Pill mode="action" onClick={() => setCreating(true)} data-testid={APPS_TEST_IDS.newApp}>
            <UiAddIcon className="size-3" />
            {APPS_COPY.newApp}
          </Pill>
        }
      />
      {apps.length === 0 ? (
        <EmptyState icon={NavAppsIcon} title={APPS_COPY.empty} size="list" />
      ) : (
        <div className={SETTINGS_LIST_CLASS}>
          {apps.map((app) => (
            <ListRow
              key={app.id}
              interactive
              onClick={() => open(app)}
              className="gap-3 px-3 py-2 text-sm"
              data-testid={`${APPS_TEST_IDS.row}-${app.slug}`}
            >
              <BoardGlyph board={{ icon: app.icon, color: app.color }} />
              <span className="min-w-0 flex-1 truncate font-medium">{app.name}</span>
              <Pill size="sm" className={app.headVersion > 0 ? `tabular-nums` : `text-muted-foreground`}>
                {versionLabel(app.headVersion)}
              </Pill>
              {(canManage(app) || app.headVersion > 0) && (
                <div onClick={(event) => event.stopPropagation()}>
                  <Menu
                    align="end"
                    aria-label={`${app.name} actions`}
                    title={app.name}
                    entries={[
                      // VAPP-121: any member exports an app that has a version.
                      ...(app.headVersion > 0
                        ? [{ kind: `item` as const, id: `export`, label: APP_EXPORT_COPY.export, icon: UiShareIcon, onSelect: () => setExporting(app) }]
                        : []),
                      ...(canManage(app)
                        ? [
                            { kind: `item` as const, id: `rename`, label: APPS_COPY.menu.rename, icon: UiEditIcon, onSelect: () => setRenaming(app) },
                            {
                              kind: `item` as const,
                              id: `delete`,
                              label: APPS_COPY.menu.delete,
                              icon: UiDeleteIcon,
                              destructive: true,
                              onSelect: () => setDeleting(app),
                            },
                          ]
                        : []),
                    ]}
                    trigger={
                      <Button variant="ghost" size="icon-sm" aria-label={`Menu for ${app.name}`}>
                        <UiMoreIcon />
                      </Button>
                    }
                  />
                </div>
              )}
            </ListRow>
          ))}
        </div>
      )}

      <AppFormDialog open={creating} onOpenChange={setCreating} teamId={teamId} teamSlug={teamSlug} />
      <AppExportDialog app={exporting} onOpenChange={(next) => !next && setExporting(null)} />
      {renaming && (
        <AppFormDialog
          key={renaming.id}
          open
          onOpenChange={(next) => !next && setRenaming(null)}
          teamId={teamId}
          teamSlug={teamSlug}
          app={renaming}
        />
      )}
      <Prompt
        open={deleting !== null}
        onOpenChange={(next) => !next && !busy && setDeleting(null)}
        title={APPS_COPY.deleteTitle.replace(`{name}`, deleting?.name ?? ``)}
        body={APPS_COPY.deleteBody}
        busy={busy}
        onDismiss={() => setDeleting(null)}
        actions={[
          { label: `Cancel`, role: `cancel` },
          { label: APPS_COPY.delete, role: `destructive`, onSelect: remove, testId: `app-delete-confirm` },
        ]}
      />
    </div>
  )
}
