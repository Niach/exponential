import { useEffect, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { APPS_COPY, APPS_RUN, APPS_TEST_IDS, screenPath } from "@exp/domain-contract/apps"
import type { AppAccessLevel } from "@exponential_app/eui/host"
import { BoardGlyph, Button, conceptIcon, EmptyState, getActionIcon, Menu, type MenuEntry, Prompt, useIsMobile } from "@exp/ui"
import { AppPreview } from "@/components/apps/app-preview"
import { AppExportDialog } from "@/components/apps/app-export-dialog"
import { HEADER_BUTTON_CLASS, MobileDetailHeader } from "@/components/team/mobile-detail-header"
import type { AppListEntry } from "@/hooks/use-apps"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import type { ScreenRequest } from "@/lib/app-screens"
import { appCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

// VAPP-242 (fixtures/apps.json THE RUN VIEW): the app as a member uses it,
// FULL SCREEN: the PUBLISHED package through THE SAME preview pieces and app
// host as the editor (records as the viewer, the published grants, the access
// level's read-only paint, the HostBanner, the consent card), with the
// `screens` shell; no toolbar. A screen tap or `app.navigate` REPLACES the
// route with `screenPath`. Phones: the detail header with the `…` menu
// (`run.menu`: Edit app, Export, Delete app); web ≥ md: no header (the
// sidebar's app row is the active one).

const UiMoreIcon = conceptIcon(`ui-more`)
const UiEditIcon = conceptIcon(`ui-edit`)
const UiShareIcon = conceptIcon(`ui-share`)
const UiDeleteIcon = conceptIcon(`ui-delete`)

export function AppRunView({
  app,
  teamId,
  teamSlug,
  me,
  canManage,
  requestedScreen,
}: {
  app: AppListEntry
  teamId: string
  teamSlug: string
  me: string | null
  /** The creator or an owner: Edit app, Delete app. */
  canManage: boolean
  requestedScreen: ScreenRequest | null
}) {
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  const [access, setAccess] = useState<AppAccessLevel>(`write`)
  useEffect(() => {
    let live = true
    trpc.appAccess.mine
      .query({ appId: app.id }, SKIP_ERROR_TOAST)
      .then((r) => live && setAccess(r.level))
      .catch(() => {})
    return () => {
      live = false
    }
  }, [app.id])
  const [exportOpen, setExportOpen] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [deleting, setDeleting] = useState(false)

  const published = app.publishedVersion > 0 ? ((app.row?.package as Record<string, unknown> | null) ?? null) : null
  const openScreen = (request: ScreenRequest) =>
    void navigate({ href: screenPath(teamSlug, app.slug, request.screen, request.params), replace: true })
  const edit = () => void navigate({ to: `/t/$teamSlug/app-editor/$appSlug`, params: { teamSlug, appSlug: app.slug } })

  const confirmDelete = async () => {
    setDeleting(true)
    try {
      const result = await trpc.apps.delete.mutate({ id: app.id }, SKIP_ERROR_TOAST)
      await appCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
      setDeleteOpen(false)
      void (isMobile
        ? navigate({ to: `/t/$teamSlug/apps`, params: { teamSlug }, replace: true })
        : navigate({ to: `/t/$teamSlug`, params: { teamSlug }, replace: true }))
    } catch (error) {
      failedAction(`delete`, `the app`, error)
    } finally {
      setDeleting(false)
    }
  }

  const surface = published ? (
    <AppPreview
      variant="run"
      pkg={published}
      phone={isMobile}
      screen={requestedScreen}
      onScreen={openScreen}
      appId={app.id}
      teamId={teamId}
      me={me}
      grantedTools={app.row?.grantedTools ?? []}
      readOnly={access === `read`}
      glyph={getActionIcon({ icon: app.icon })}
    />
  ) : (
    <div className="flex min-h-0 flex-1 items-center justify-center" data-testid={APPS_TEST_IDS.run}>
      <EmptyState icon={getActionIcon({ icon: app.icon })} title={APPS_COPY.runEmpty} size="page" />
    </div>
  )

  const dialogs = (
    <>
      <AppExportDialog app={exportOpen ? app : null} onOpenChange={setExportOpen} />
      <Prompt
        open={deleteOpen}
        onOpenChange={(next) => !next && !deleting && setDeleteOpen(false)}
        title={APPS_COPY.deleteTitle.replace(`{name}`, app.name)}
        body={APPS_COPY.deleteBody}
        busy={deleting}
        onDismiss={() => setDeleteOpen(false)}
        actions={[
          { label: `Cancel`, role: `cancel` },
          { label: APPS_COPY.delete, role: `destructive`, onSelect: confirmDelete, testId: `app-delete-confirm` },
        ]}
      />
    </>
  )

  if (!isMobile) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        {surface}
        {dialogs}
      </div>
    )
  }

  const items: Record<string, MenuEntry | null> = {
    edit: canManage ? { kind: `item`, id: `edit`, label: APPS_COPY.menu.edit, icon: UiEditIcon, onSelect: edit } : null,
    export: app.publishedVersion > 0 ? { kind: `item`, id: `export`, label: APPS_COPY.menu.export, icon: UiShareIcon, onSelect: () => setExportOpen(true) } : null,
    delete: canManage
      ? { kind: `item`, id: `delete`, label: APPS_COPY.menu.delete, icon: UiDeleteIcon, destructive: true, onSelect: () => setDeleteOpen(true) }
      : null,
  }
  const entries = APPS_RUN.menu.map((id) => items[id]).filter((entry): entry is MenuEntry => entry !== null)

  return (
    <div className="flex h-dvh min-h-0 flex-col">
      <MobileDetailHeader
        title={
          <span className="flex min-w-0 items-center justify-center gap-2">
            <BoardGlyph board={{ icon: app.icon, color: app.color }} className="size-5" />
            <span className="truncate">{app.name}</span>
          </span>
        }
        onBack={() => void navigate({ to: `/t/$teamSlug/apps`, params: { teamSlug } })}
        menu={
          entries.length > 0 ? (
            <Menu
              align="end"
              aria-label={`${app.name} actions`}
              title={app.name}
              entries={entries}
              trigger={
                <Button variant="ghost" size="icon-sm" className={HEADER_BUTTON_CLASS} aria-label={`Menu for ${app.name}`} data-testid={APPS_TEST_IDS.runMenu}>
                  <UiMoreIcon />
                </Button>
              }
            />
          ) : undefined
        }
      />
      {surface}
      {dialogs}
    </div>
  )
}
