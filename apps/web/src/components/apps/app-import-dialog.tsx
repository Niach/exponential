import { useEffect, useRef, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { APP_EXPORT_COPY, APP_EXPORT_LIMITS, type ImportIssue } from "@exp/domain-contract/app-export"
import { Button, Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle, Textarea } from "@exp/ui"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { appCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

// VAPP-121 (fixtures/app-export.json): the IMPORT step. VAPP-176: its own
// dialog, opened from the Apps list band's Import pill (New app has no
// dialog any more). Paste or choose an app file; the server lists its issues
// as `path: message` rows; Import creates the app and opens its editor.

export function AppImportDialog({
  open,
  onOpenChange,
  teamId,
  teamSlug,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  teamId: string
  teamSlug: string
}) {
  const navigate = useNavigate()
  const [text, setText] = useState(``)
  const [issues, setIssues] = useState<ImportIssue[] | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const fileInput = useRef<HTMLInputElement>(null)

  // The server is the one authority: a debounced check as the text changes.
  useEffect(() => {
    if (!open) return
    if (!text.trim()) return void setIssues(null)
    let live = true
    const timer = setTimeout(() => {
      trpc.apps.validateImport
        .mutate({ teamId, text }, SKIP_ERROR_TOAST)
        .then((result) => live && setIssues(result.issues))
        .catch(() => live && setIssues([{ path: `/`, message: `could not be checked` }]))
    }, 300)
    return () => {
      live = false
      clearTimeout(timer)
    }
  }, [open, text, teamId])

  const reset = () => {
    setText(``)
    setIssues(null)
  }

  const runImport = async () => {
    if (submitting || !issues || issues.length > 0) return
    setSubmitting(true)
    try {
      const result = await trpc.apps.import.mutate({ teamId, text }, SKIP_ERROR_TOAST)
      await appCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
      reset()
      onOpenChange(false)
      void navigate({ to: `/t/$teamSlug/apps/$appSlug`, params: { teamSlug, appSlug: result.app.slug } })
    } catch (error) {
      failedAction(`import`, `the app`, error)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) reset()
        onOpenChange(next)
      }}
    >
      <DialogContent className="sm:max-w-md" data-testid="app-import-dialog">
        <DialogHeader>
          <DialogTitle>{APP_EXPORT_COPY.importTitle}</DialogTitle>
        </DialogHeader>
        <div className="flex min-h-0 flex-1 flex-col gap-4">
          <DialogBody className="space-y-3">
            <Textarea
              value={text}
              onChange={(event) => setText(event.target.value)}
              placeholder={APP_EXPORT_COPY.importPlaceholder}
              rows={8}
              autoFocus
              spellCheck={false}
              className="font-mono text-xs"
              data-testid="app-import-text"
            />
            <input
              ref={fileInput}
              type="file"
              accept=".json,application/json"
              className="hidden"
              onChange={(event) => void event.target.files?.[0]?.text().then(setText)}
            />
            <Button type="button" variant="outline" size="sm" onClick={() => fileInput.current?.click()} data-testid="app-import-file">
              {APP_EXPORT_COPY.chooseFile}
            </Button>
            {issues && issues.length > 0 && (
              <ul className="space-y-1 font-mono text-xs text-destructive" data-testid="app-import-issues">
                {issues.slice(0, APP_EXPORT_LIMITS.issuesShown).map((issue, i) => (
                  <li key={i} className="break-words">{`${issue.path}: ${issue.message}`}</li>
                ))}
                {issues.length > APP_EXPORT_LIMITS.issuesShown && (
                  <li>{APP_EXPORT_COPY.moreIssues.replace(`{n}`, String(issues.length - APP_EXPORT_LIMITS.issuesShown))}</li>
                )}
              </ul>
            )}
          </DialogBody>
          <DialogFooter>
            <Button onClick={() => void runImport()} disabled={!issues || issues.length > 0} pending={submitting} data-testid="app-import-confirm">
              {APP_EXPORT_COPY.importAction}
            </Button>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  )
}
