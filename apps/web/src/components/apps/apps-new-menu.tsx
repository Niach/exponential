import { useState, type ReactElement } from "react"
import { APPS_COPY, APPS_RUN, APPS_TEST_IDS } from "@exp/domain-contract/apps"
import { conceptIcon, Menu, type MenuEntry } from "@exp/ui"
import { AppImportDialog } from "@/components/apps/app-import-dialog"
import { useNewApp } from "@/hooks/use-new-app"

// VAPP-242 (fixtures/apps.json THE NEW MENU): every apps `+` (the sidebar
// group's, the rail's, the phone list band's pill) opens THE Menu with
// `run.newMenu`: New app (a draft and its editor), then Import… (the import
// dialog; an imported app opens its editor).

const UiAddIcon = conceptIcon(`ui-add`)
const UiFileIcon = conceptIcon(`ui-file`)

export function AppsNewMenu({ teamId, teamSlug, trigger, align = `end` }: { teamId: string; teamSlug: string; trigger: ReactElement; align?: `start` | `end` }) {
  const newApp = useNewApp(teamId, teamSlug)
  const [importing, setImporting] = useState(false)
  const items: Record<string, MenuEntry> = {
    newApp: { kind: `item`, id: `newApp`, label: APPS_COPY.newApp, icon: UiAddIcon, disabled: newApp.pending, onSelect: () => void newApp.create() },
    import: { kind: `item`, id: `import`, label: APPS_COPY.import, icon: UiFileIcon, onSelect: () => setImporting(true), "data-testid": APPS_TEST_IDS.importApp },
  }
  return (
    <>
      <Menu
        align={align}
        aria-label={APPS_COPY.newApp}
        title={APPS_COPY.title}
        data-testid={APPS_TEST_IDS.newMenu}
        entries={APPS_RUN.newMenu.map((id) => items[id]!)}
        trigger={trigger}
      />
      <AppImportDialog open={importing} onOpenChange={setImporting} teamId={teamId} teamSlug={teamSlug} />
    </>
  )
}
