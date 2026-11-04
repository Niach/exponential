import { useEffect, useRef, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { APP_EXPORT_COPY, APP_EXPORT_LIMITS, type ImportIssue } from "@exp/domain-contract/app-export"
import { APP_TEMPLATES, APP_TEMPLATES_COPY, APP_TEMPLATES_TEST_IDS } from "@exp/domain-contract/app-templates"
import { APPS_COPY, APPS_TEST_IDS, UPSTREAM_PICKER_NONE } from "@exp/domain-contract/apps"
import {
  BoardGlyph,
  Button,
  Dialog,
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  GlassGroup,
  GlassSectionHeader,
  ListRow,
  Switch,
  Textarea,
} from "@exp/ui"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { appCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { UpstreamPickerSelect } from "./upstream-picker-select"

type ImportCheck = Awaited<ReturnType<typeof trpc.apps.validateImport.mutate>>

// VAPP-121 (fixtures/app-export.json): the IMPORT step. VAPP-176: its own
// dialog, opened from the Apps list band's Import pill (New app has no
// dialog any more). Paste or choose an app file; the server lists its issues
// as `path: message` rows; Import creates the app and opens its editor.
// VAPP-234 (fixtures/app-templates.json): the TEMPLATES step above it: a
// tap on a signed template imports it at once (v1 published, the sample
// records only with the switch on) and opens its editor.
// #VAPP-254 (apps.json `upstreamPickerCases`): an owner picks which team
// server fills each upstream the file declares.

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
  const [check, setCheck] = useState<Pick<ImportCheck, `upstreams` | `servers`>>({ upstreams: [], servers: [] })
  const [picks, setPicks] = useState<Record<string, string>>({})
  const [submitting, setSubmitting] = useState(false)
  const [withRecords, setWithRecords] = useState(false)
  const [picking, setPicking] = useState<string | null>(null)
  const fileInput = useRef<HTMLInputElement>(null)

  // The server is the one authority: a debounced check as the text changes.
  useEffect(() => {
    if (!open) return
    if (!text.trim()) {
      setCheck({ upstreams: [], servers: [] })
      return void setIssues(null)
    }
    let live = true
    const timer = setTimeout(() => {
      trpc.apps.validateImport
        .mutate({ teamId, text }, SKIP_ERROR_TOAST)
        .then((result) => {
          if (!live) return
          setIssues(result.issues)
          setCheck(result)
        })
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
    setCheck({ upstreams: [], servers: [] })
    setPicks({})
    setWithRecords(false)
  }

  const opened = (slug: string) => {
    reset()
    onOpenChange(false)
    void navigate({ to: `/t/$teamSlug/app-editor/$appSlug`, params: { teamSlug, appSlug: slug } })
  }

  const pickTemplate = async (template: string) => {
    if (picking || submitting) return
    setPicking(template)
    try {
      const result = await trpc.apps.importTemplate.mutate({ teamId, template, includeRecords: withRecords }, SKIP_ERROR_TOAST)
      await appCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
      opened(result.app.slug)
    } catch (error) {
      failedAction(`import`, `the template`, error)
    } finally {
      setPicking(null)
    }
  }

  const runImport = async () => {
    if (submitting || !issues || issues.length > 0) return
    setSubmitting(true)
    try {
      const upstreamMappings = check.upstreams.flatMap(({ name }) => (picks[name] && picks[name] !== UPSTREAM_PICKER_NONE ? [{ upstream: name, serverId: picks[name] }] : []))
      const result = await trpc.apps.import.mutate({ teamId, text, upstreamMappings }, SKIP_ERROR_TOAST)
      await appCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
      opened(result.app.slug)
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
            <GlassSectionHeader label={APP_TEMPLATES_COPY.title} />
            <GlassGroup>
              {APP_TEMPLATES.map((template) => (
                <ListRow
                  key={template.id}
                  interactive={picking === null}
                  onClick={() => void pickTemplate(template.id)}
                  aria-disabled={picking !== null}
                  className="gap-3 px-4 py-2.5 text-sm"
                  data-testid={`${APP_TEMPLATES_TEST_IDS.row}-${template.id}`}
                >
                  <BoardGlyph board={template} />
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className="truncate font-medium">{template.name}</span>
                    <span className="truncate text-xs text-muted-foreground">{template.description}</span>
                  </span>
                </ListRow>
              ))}
            </GlassGroup>
            <GlassGroup>
              <label className="flex items-center justify-between gap-3 px-4 py-3 text-sm">
                {APP_TEMPLATES_COPY.includeRecords}
                <Switch checked={withRecords} onCheckedChange={setWithRecords} disabled={picking !== null} data-testid={APP_TEMPLATES_TEST_IDS.records} />
              </label>
            </GlassGroup>
            <Textarea
              value={text}
              onChange={(event) => setText(event.target.value)}
              placeholder={APP_EXPORT_COPY.importPlaceholder}
              rows={6}
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
            {issues?.length === 0 && check.upstreams.length > 0 && check.servers.length > 0 && (
              <div className="space-y-1" data-testid={APPS_TEST_IDS.importUpstreams}>
                <GlassSectionHeader label={APPS_COPY.accounts.title} />
                <GlassGroup>
                  {check.upstreams.map((upstream) => {
                    const picked = picks[upstream.name]
                    const server = picked ? (check.servers.find((s) => s.id === picked) ?? null) : null
                    const row = picked ? { name: upstream.name, serverId: server?.id ?? null, serverName: server?.name ?? null } : upstream
                    return (
                      <div key={upstream.name} className="flex items-center gap-3 px-4 py-2.5" data-testid={`${APPS_TEST_IDS.importUpstream}-${upstream.name}`}>
                        <div className="min-w-0 flex-1 truncate text-sm">{upstream.name}</div>
                        <UpstreamPickerSelect row={row} servers={check.servers} disabled={submitting} testId={`${APPS_TEST_IDS.importUpstream}-${upstream.name}-picker`} onPick={(value) => setPicks((prev) => ({ ...prev, [upstream.name]: value }))} />
                      </div>
                    )
                  })}
                </GlassGroup>
              </div>
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
