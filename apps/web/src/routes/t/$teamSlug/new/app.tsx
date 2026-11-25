import { useEffect, useState } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { APPS_COPY, APPS_RUN, APPS_TEST_IDS } from "@exp/domain-contract/apps"
import { conceptIcon, ListRow, SETTINGS_LIST_CLASS, Separator } from "@exp/ui"
import { AppImportForm } from "@/components/apps/app-import-dialog"
import { NewPageHeading } from "@/components/team/new-page-heading"
import { PAGE_END_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { useNewApp } from "@/hooks/use-new-app"
import { useSettingsPage } from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

// #EXP-2027 (sidebar-tree.json TEAM MENU): New › App opens this PAGE: the
// apps New menu's entries (apps.json `run.newMenu`) as rows. New app makes
// the draft and opens its editor in this page's place (VAPP-176); Import…
// reveals the import flow (templates, a file, pasted text) below the rows,
// in the page, never a dialog (`AppImportForm`, the import dialog's body); an
// imported app opens its editor in this page's place. Owner-only: anyone
// else lands on Home.
export const Route = createFileRoute(`/t/$teamSlug/new/app`)({
  head: () => ({ meta: [{ title: pageTitle(APPS_COPY.newApp) }] }),
  component: NewAppRoute,
})

const UiAddIcon = conceptIcon(`ui-add`)
const UiFileIcon = conceptIcon(`ui-file`)

function NewAppRoute() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  const newApp = useNewApp(team?.id, teamSlug, { replace: true })
  const [importing, setImporting] = useState(false)
  const refused = resolved && !permissions.isOwner
  useEffect(() => {
    if (refused) void navigate({ to: `/home`, replace: true })
  }, [refused, navigate])
  if (!team || !resolved || refused) return null
  const entries = {
    newApp: { label: APPS_COPY.newApp, icon: UiAddIcon, testId: undefined, onSelect: () => void newApp.create() },
    import: { label: APPS_COPY.import, icon: UiFileIcon, testId: APPS_TEST_IDS.importApp, onSelect: () => setImporting((open) => !open) },
  } as const
  return (
    <div className={`mx-auto w-full max-w-4xl space-y-6 p-4 sm:p-6 ${PAGE_END_CLEARANCE}`}>
      <NewPageHeading teamSlug={teamSlug}>{APPS_COPY.newApp}</NewPageHeading>
      <div className={SETTINGS_LIST_CLASS}>
        {APPS_RUN.newMenu.map((id) => {
          const entry = entries[id as keyof typeof entries]
          if (!entry) return null
          const Icon = entry.icon
          const disabled = id === `newApp` && newApp.pending
          return (
            <ListRow
              key={id}
              interactive
              active={id === `import` && importing}
              aria-expanded={id === `import` ? importing : undefined}
              aria-disabled={disabled || undefined}
              onClick={disabled ? undefined : entry.onSelect}
              className="gap-3 px-3 py-2 text-sm"
              data-testid={entry.testId ?? `new-app-${id}`}
            >
              <Icon className="size-4 shrink-0 text-muted-foreground" />
              <span className="min-w-0 flex-1 truncate">{entry.label}</span>
            </ListRow>
          )
        })}
      </div>
      {importing && (
        <>
          <Separator />
          <AppImportForm
            layout="page"
            teamId={team.id}
            onImported={(slug) => void navigate({ to: `/t/$teamSlug/app-editor/$appSlug`, params: { teamSlug, appSlug: slug }, replace: true })}
          />
        </>
      )}
    </div>
  )
}
