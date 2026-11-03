import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { APPS_COPY, APPS_TEST_IDS, listPill } from "@exp/domain-contract/apps"
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
import { AppsNewMenu } from "@/components/apps/apps-new-menu"
import type { AppListEntry } from "@/hooks/use-apps"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { appCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

// VAPP-110 (fixtures/apps.json THE LIST): the Apps list = the band with
// `New app` as its trailing pill (VAPP-242: THE NEW MENU, New app / Import…),
// then one ListRow per app (glyph, name, the version pill, the `…` menu:
// Edit app, Export, Delete). VAPP-242: PHONES ONLY (the Apps tab); a row
// RUNS the app (THE RUN VIEW), the editor sits behind `…` › Edit app.

const NavAppsIcon = conceptIcon(`nav-apps`)
const UiAddIcon = conceptIcon(`ui-add`)
const UiMoreIcon = conceptIcon(`ui-more`)
const UiShareIcon = conceptIcon(`ui-share`)
const UiDeleteIcon = conceptIcon(`ui-delete`)
const UiEditIcon = conceptIcon(`ui-edit`)

export function AppsList({
  teamId,
  teamSlug,
  apps,
  me,
  isOwner,
}: {
  teamId: string
  teamSlug: string
  apps: readonly AppListEntry[]
  me: string | null
  isOwner: boolean
}) {
  const navigate = useNavigate()
  const [exporting, setExporting] = useState<AppListEntry | null>(null)
  const [deleting, setDeleting] = useState<AppListEntry | null>(null)
  const [busy, setBusy] = useState(false)

  const open = (app: AppListEntry) => void navigate({ to: `/t/$teamSlug/apps/$appSlug`, params: { teamSlug, appSlug: app.slug } })
  const edit = (app: AppListEntry) => void navigate({ to: `/t/$teamSlug/app-editor/$appSlug`, params: { teamSlug, appSlug: app.slug } })

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
          <AppsNewMenu
            teamId={teamId}
            teamSlug={teamSlug}
            trigger={
              <Pill mode="action" data-testid={APPS_TEST_IDS.newApp}>
                <UiAddIcon className="size-3" />
                {APPS_COPY.newApp}
              </Pill>
            }
          />
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
              <Pill size="sm" className={app.publishedVersion > 0 ? `tabular-nums` : `text-muted-foreground`}>
                {listPill(app)}
              </Pill>
              {(canManage(app) || app.publishedVersion > 0) && (
                <div onClick={(event) => event.stopPropagation()}>
                  <Menu
                    align="end"
                    aria-label={`${app.name} actions`}
                    title={app.name}
                    entries={[
                      // VAPP-242: the editor sits here (the creator or an owner).
                      ...(canManage(app)
                        ? [{ kind: `item` as const, id: `edit`, label: APPS_COPY.menu.edit, icon: UiEditIcon, onSelect: () => edit(app) }]
                        : []),
                      // VAPP-121: any member exports an app that has a published version.
                      ...(app.publishedVersion > 0
                        ? [{ kind: `item` as const, id: `export`, label: APP_EXPORT_COPY.export, icon: UiShareIcon, onSelect: () => setExporting(app) }]
                        : []),
                      ...(canManage(app)
                        ? [
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

      <AppExportDialog app={exporting} onOpenChange={(next) => !next && setExporting(null)} />
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
