import { useEffect, useRef, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import type { BoardIcon } from "@exp/db-schema/domain"
import { APPS_COPY, APP_DESCRIPTION_MAX } from "@exp/domain-contract/apps"
import { APP_EXPORT_COPY, APP_EXPORT_LIMITS, type ImportIssue } from "@exp/domain-contract/app-export"
import {
  Button,
  Dialog,
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  GlassGroup,
  InlinePickerRow,
  Textarea,
} from "@exp/ui"
import { BoardIdentityRow } from "@/components/board-form-fields"
import type { AppListEntry } from "@/hooks/use-apps"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { appCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

// VAPP-110 (fixtures/apps.json): New app = the board form's identity row
// (icon + colour + name) and an optional description; Create opens the
// editor. The same dialog renames an existing app (`app` set).
// VAPP-121 (fixtures/app-export.json): New app's `Import…` switches it to
// the import step: paste or choose an app file; the server lists its issues
// as `path: message` rows; Import creates the app and opens its editor.

const DEFAULT_COLOR = `#6366f1`
const DEFAULT_ICON: BoardIcon = `puzzle`

export function AppFormDialog({
  open,
  onOpenChange,
  teamId,
  teamSlug,
  app,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  teamId: string
  teamSlug: string
  /** Rename mode: the app to edit. */
  app?: AppListEntry | null
}) {
  const navigate = useNavigate()
  const [name, setName] = useState(app?.name ?? ``)
  const [icon, setIcon] = useState<BoardIcon>((app?.icon as BoardIcon | null) ?? DEFAULT_ICON)
  const [color, setColor] = useState(app?.color ?? DEFAULT_COLOR)
  const [description, setDescription] = useState(app?.description ?? ``)
  const [submitting, setSubmitting] = useState(false)
  const editing = Boolean(app)
  const [importing, setImporting] = useState(false)
  const [importText, setImportText] = useState(``)
  const [issues, setIssues] = useState<ImportIssue[] | null>(null)
  const fileInput = useRef<HTMLInputElement>(null)

  // The server is the one authority: a debounced check as the text changes.
  useEffect(() => {
    if (!importing) return
    if (!importText.trim()) return void setIssues(null)
    let live = true
    const timer = setTimeout(() => {
      trpc.apps.validateImport
        .mutate({ teamId, text: importText }, SKIP_ERROR_TOAST)
        .then((result) => live && setIssues(result.issues))
        .catch(() => live && setIssues([{ path: `/`, message: `could not be checked` }]))
    }, 300)
    return () => {
      live = false
      clearTimeout(timer)
    }
  }, [importing, importText, teamId])

  const chooseFile = async (file: File | undefined) => {
    if (!file) return
    setImportText(await file.text())
  }

  const runImport = async () => {
    if (submitting || !issues || issues.length > 0) return
    setSubmitting(true)
    try {
      const result = await trpc.apps.import.mutate({ teamId, text: importText }, SKIP_ERROR_TOAST)
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

  const reset = () => {
    setName(app?.name ?? ``)
    setIcon((app?.icon as BoardIcon | null) ?? DEFAULT_ICON)
    setColor(app?.color ?? DEFAULT_COLOR)
    setDescription(app?.description ?? ``)
    setImporting(false)
    setImportText(``)
    setIssues(null)
  }

  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    if (submitting || !name.trim()) return
    setSubmitting(true)
    try {
      if (app) {
        const result = await trpc.apps.update.mutate(
          { id: app.id, name: name.trim(), icon, color, description },
          SKIP_ERROR_TOAST
        )
        await appCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
        onOpenChange(false)
      } else {
        const result = await trpc.apps.create.mutate(
          { teamId, name: name.trim(), icon, color, description: description.trim() || undefined },
          SKIP_ERROR_TOAST
        )
        await appCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
        reset()
        onOpenChange(false)
        void navigate({ to: `/t/$teamSlug/apps/$appSlug`, params: { teamSlug, appSlug: result.app.slug } })
      }
    } catch (error) {
      failedAction(editing ? `rename` : `create`, `the app`, error)
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
      <DialogContent className="sm:max-w-md" data-testid="app-form-dialog">
        <DialogHeader>
          <DialogTitle>{importing ? APP_EXPORT_COPY.importTitle : editing ? APPS_COPY.menu.rename : APPS_COPY.newApp}</DialogTitle>
        </DialogHeader>
        {importing ? (
          <div className="flex min-h-0 flex-1 flex-col gap-4">
            <DialogBody className="space-y-3">
              <Textarea
                value={importText}
                onChange={(event) => setImportText(event.target.value)}
                placeholder={APP_EXPORT_COPY.importPlaceholder}
                rows={8}
                autoFocus
                spellCheck={false}
                className="font-mono text-xs"
                data-testid="app-import-text"
              />
              <input ref={fileInput} type="file" accept=".json,application/json" className="hidden" onChange={(event) => void chooseFile(event.target.files?.[0])} />
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
        ) : (
        <form onSubmit={submit} className="flex min-h-0 flex-1 flex-col gap-4">
          <DialogBody className="space-y-4">
            {/* N-14: THE row host: the icon grid opens under the whole
                identity row, so no sibling moves (picker-presentation.json). */}
            <GlassGroup>
              <InlinePickerRow>
                <BoardIdentityRow
                  name={name}
                  onNameChange={setName}
                  autoFocus
                  icon={icon}
                  onIconChange={setIcon}
                  color={color}
                  onColorChange={setColor}
                  placeholder={APPS_COPY.namePlaceholder}
                />
              </InlinePickerRow>
            </GlassGroup>
            <Textarea
              value={description}
              onChange={(event) => setDescription(event.target.value.slice(0, APP_DESCRIPTION_MAX))}
              placeholder={APPS_COPY.descriptionPlaceholder}
              rows={3}
              data-testid="app-form-description"
            />
          </DialogBody>
          <DialogFooter>
            {!editing && (
              <Button type="button" variant="ghost" className="mr-auto" onClick={() => setImporting(true)} data-testid="app-import">
                {APP_EXPORT_COPY.import}
              </Button>
            )}
            <Button type="submit" disabled={!name.trim()} pending={submitting}>
              {editing ? APPS_COPY.rename : APPS_COPY.create}
            </Button>
          </DialogFooter>
        </form>
        )}
      </DialogContent>
    </Dialog>
  )
}
