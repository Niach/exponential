import { useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { APP_EXPORT_COPY, APP_EXPORT_MIME, appExportFileName, appExportText, buildAppExport } from "@exp/domain-contract/app-export"
import { Button, Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle, GlassGroup, Switch } from "@exp/ui"
import type { AppListEntry } from "@/hooks/use-apps"
import { failedAction } from "@/lib/action-error"
import { appRecordCollection } from "@/lib/collections"

// VAPP-121 (fixtures/app-export.json): Export = ONE dialog titled
// `copy.exportTitle`, a single "Include records" switch row (only when the
// app has records, off by default) and the Export button, which downloads
// `<slug>.app.json`: the identity, the head package and, switched on, the
// records as {kind, data}. Built on the client from the synced rows.

/** Hand a text file to the browser as a download. */
function downloadText(name: string, text: string) {
  const url = URL.createObjectURL(new Blob([text], { type: APP_EXPORT_MIME }))
  const a = document.createElement(`a`)
  a.href = url
  a.download = name
  document.body.appendChild(a)
  a.click()
  a.remove()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}

export function AppExportDialog({ app, onOpenChange }: { app: AppListEntry | null; onOpenChange: (open: boolean) => void }) {
  const [includeRecords, setIncludeRecords] = useState(false)
  const { data: records } = useLiveQuery(
    (query) => (app ? query.from({ r: appRecordCollection }).where(({ r }) => eq(r.appId, app.id)) : undefined),
    [app?.id]
  )
  const count = records?.length ?? 0
  const pkg = (app?.row?.package ?? null) as Record<string, unknown> | null

  const exportFile = () => {
    if (!app || !pkg) return
    try {
      const file = buildAppExport({
        app,
        package: pkg,
        records: (records ?? []).map((r) => ({ id: r.id, kind: r.kind, data: r.data as Record<string, unknown>, createdAt: r.createdAt })),
        includeRecords,
      })
      downloadText(appExportFileName(app.slug), appExportText(file))
      onOpenChange(false)
    } catch (error) {
      failedAction(`export`, `the app`, error)
    }
  }

  return (
    <Dialog
      open={app !== null}
      onOpenChange={(next) => {
        if (!next) setIncludeRecords(false)
        onOpenChange(next)
      }}
    >
      <DialogContent className="sm:max-w-md" data-testid="app-export-dialog">
        <DialogHeader>
          <DialogTitle>{APP_EXPORT_COPY.exportTitle.replace(`{name}`, app?.name ?? ``)}</DialogTitle>
        </DialogHeader>
        {count > 0 && (
          <DialogBody>
            <GlassGroup>
              <label className="flex items-center justify-between gap-3 px-4 py-3 text-sm">
                {APP_EXPORT_COPY.includeRecords}
                <Switch checked={includeRecords} onCheckedChange={setIncludeRecords} data-testid="app-export-records" />
              </label>
            </GlassGroup>
          </DialogBody>
        )}
        <DialogFooter>
          <Button onClick={exportFile} disabled={!pkg} data-testid="app-export-confirm">
            {APP_EXPORT_COPY.export}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
