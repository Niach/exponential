import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import type { DiffFile } from "@exp/domain-contract/diff"
import {
  AgentRunMark,
  Button,
  LiveDot,
  Pill,
  SessionResultsView,
  conceptIcon,
  parseSessionResultGroups,
} from "@exp/ui"
import { signedAttachmentUrl, useMcpActions } from "./actions"
import { Markdown } from "./markdown"
import { prConcept, runIsWorking, runSubject, runTone, type RunDetail } from "./model"
import { RunChanges } from "./run-changes"
import {
  RUN_POLL_MS,
  diffFromPrFiles,
  resumedRunId,
  runAttachmentIds,
  runIsLive,
  runMarkState,
  runPillLabel,
  runResultRecords,
  runWorkingCaption,
  type RunFace,
} from "./run-model"
import { RunSteerComposer } from "./run-steer"

// EXP-1183 — `exponential_sessions_get` as the app's run / Review page: the
// work header (agent mark, subject, state, PR, a quiet "Open in Exponential"),
// the report as the Guide through the real `SessionResultsView` (pictures
// resolved to signed URLs first: `/api/attachments` needs a session the
// sandboxed frame has not), the PR's diff through the real `FileDiffList`,
// and — while the run is live — Stop plus a steer composer; an ended run
// offers Resume and switches to the new run. A live run re-reads itself
// every 8s.
const BackIcon = conceptIcon(`ui-back`)
const ExternalIcon = conceptIcon(`ui-external-link`)
const StopIcon = conceptIcon(`coding-stop`)
const ResumeIcon = conceptIcon(`run-resume`)

export function RunView({
  run: initial,
  onBack,
  onOpenLink,
}: {
  run: RunDetail
  onBack?: () => void
  onOpenLink?: (url: string) => void
}) {
  const { call } = useMcpActions()
  const [run, setRun] = useState<RunFace>(initial)
  const [actionError, setActionError] = useState<string | null>(null)
  const mounted = useRef(true)
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])
  useEffect(() => setRun(initial), [initial])

  const refresh = useCallback(
    async (id: string) => {
      const result = await call<RunFace>(`exponential_sessions_get`, { id })
      if (!mounted.current) return
      if (result.kind === `ok`) setRun(result.data)
      return result
    },
    [call]
  )

  // Live refresh: state, caption and new results while the run is up.
  const live = runIsLive(run)
  useEffect(() => {
    if (!live) return
    const timer = setInterval(() => void refresh(run.id), RUN_POLL_MS)
    return () => clearInterval(timer)
  }, [live, run.id, refresh])

  // Pictures: every attachment id → its signed URL, before they render.
  const attachmentKey = runAttachmentIds(run.results).join(`,`)
  const [signed, setSigned] = useState<Record<string, string | null>>({})
  useEffect(() => {
    const ids = attachmentKey ? attachmentKey.split(`,`) : []
    if (ids.length === 0) return
    let current = true
    void Promise.all(ids.map((id) => signedAttachmentUrl(call, id))).then((urls) => {
      if (!current) return
      setSigned((prev) => {
        const next = { ...prev }
        ids.forEach((id, index) => {
          next[id] = urls[index]
        })
        return next
      })
    })
    return () => {
      current = false
    }
  }, [attachmentKey, call])

  // The PR diff: re-read when the PR or the report changes (a new push
  // usually comes with a new report).
  const [files, setFiles] = useState<DiffFile[]>([])
  const [filesLoading, setFilesLoading] = useState(false)
  const [filesError, setFilesError] = useState<string | null>(null)
  const hasPr = Boolean(run.issueId && run.prUrl)
  const filesKey = hasPr
    ? `${run.issueId}|${run.prUrl}|${run.prState}|${run.results?.length ?? 0}`
    : ``
  useEffect(() => {
    if (!filesKey || !run.issueId) {
      setFiles([])
      return
    }
    let current = true
    setFilesLoading(true)
    void call(`exponential_issues_pr_files`, { issueId: run.issueId }).then((result) => {
      if (!current) return
      setFilesLoading(false)
      if (result.kind === `error`) {
        setFilesError(result.message)
        return
      }
      setFilesError(null)
      setFiles(diffFromPrFiles(result.data))
    })
    return () => {
      current = false
    }
  }, [filesKey, run.issueId, call])

  const groups = useMemo(
    () => parseSessionResultGroups(runResultRecords(run.results, signed)),
    [run.results, signed]
  )

  // A Guide file row opens that file's card below (null first, so a re-pick
  // of the same path scrolls again).
  const [focusPath, setFocusPath] = useState<string | null>(null)
  const openFile = (path: string) => {
    setFocusPath(null)
    requestAnimationFrame(() => setFocusPath(path))
  }

  // Stop (confirmed inline) / Resume.
  const [confirmStop, setConfirmStop] = useState(false)
  const [busy, setBusy] = useState(false)
  const stop = async () => {
    setBusy(true)
    setActionError(null)
    const result = await call(`exponential_sessions_kill`, { id: run.id })
    if (!mounted.current) return
    setBusy(false)
    setConfirmStop(false)
    if (result.kind === `error`) setActionError(result.message)
    else await refresh(run.id)
  }
  const resume = async () => {
    if (!run.deviceId) return
    setBusy(true)
    setActionError(null)
    const result = await call(`exponential_sessions_start`, {
      deviceId: run.deviceId,
      resumeSessionId: run.id,
    })
    if (!mounted.current) return
    const nextId = result.kind === `ok` ? resumedRunId(result.data) : null
    if (result.kind === `error`) setActionError(result.message)
    else if (!nextId) setActionError(`The device did not report the resumed run.`)
    else {
      const next = await refresh(nextId)
      if (next?.kind === `error`) setActionError(next.message)
    }
    if (mounted.current) setBusy(false)
  }
  const merge = async (): Promise<string | null> => {
    if (!run.issueId) return null
    const result = await call<{ results?: { merged?: boolean; error?: string; queued?: boolean }[] }>(
      `exponential_pr_merge`,
      { issueId: run.issueId }
    )
    if (result.kind === `error`) return result.message
    const first = result.data.results?.[0]
    if (first && !first.merged && !first.queued) return first.error ?? `The merge did not go through.`
    await refresh(run.id)
    return null
  }

  const pr = prConcept(run.prState)
  const PrIcon = pr ? conceptIcon(pr) : null
  const caption = runWorkingCaption(run)

  return (
    <article className="flex min-h-full flex-col" data-testid="run-view">
      <header className="flex flex-col gap-2 px-7 pt-3 pb-1 md:px-9">
        <div className="flex min-w-0 items-center gap-2">
          {onBack && (
            <Button
              variant="ghost"
              size="icon-xs"
              className="-ml-1.5"
              aria-label="Back to the list"
              onClick={onBack}
            >
              <BackIcon />
            </Button>
          )}
          <AgentRunMark
            agent={run.agent}
            state={runMarkState(run)}
            needsYou={Boolean(run.needsInput)}
            ringClassName="ring-background"
          />
          <h1 className="min-w-0 flex-1 truncate text-base font-semibold" title={runSubject(run)}>
            {runSubject(run)}
          </h1>
          {run.url && onOpenLink && (
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label="Open in Exponential"
              title="Open in Exponential"
              onClick={() => onOpenLink(run.url as string)}
            >
              <ExternalIcon />
            </Button>
          )}
          {live &&
            (confirmStop ? (
              <span className="flex items-center gap-1">
                <Button variant="ghost" size="xs" disabled={busy} onClick={() => setConfirmStop(false)}>
                  Cancel
                </Button>
                <Button variant="glass" size="xs" disabled={busy} onClick={() => void stop()}>
                  <StopIcon />
                  {busy ? `Stopping…` : `Stop run`}
                </Button>
              </span>
            ) : (
              <Button variant="ghost" size="xs" onClick={() => setConfirmStop(true)}>
                <StopIcon />
                Stop
              </Button>
            ))}
          {!live && run.deviceId && (
            <Button variant="ghost" size="xs" disabled={busy} onClick={() => void resume()}>
              <ResumeIcon />
              {busy ? `Resuming…` : `Resume`}
            </Button>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-1.5">
          <Pill leading={<LiveDot tone={runTone(run)} ping={runIsWorking(run)} />}>
            {runPillLabel(run)}
          </Pill>
          {PrIcon && run.prUrl && (
            <Pill
              mode="action"
              leading={<PrIcon />}
              onClick={() => onOpenLink?.(run.prUrl as string)}
            >
              {run.prNumber != null ? `#${run.prNumber}` : `Pull request`}
            </Pill>
          )}
          {run.branch && (
            <span className="truncate font-mono text-xs text-muted-foreground">{run.branch}</span>
          )}
        </div>
        {caption && (
          <p className="truncate text-xs text-muted-foreground" data-testid="run-caption">
            {caption}
          </p>
        )}
        {actionError && <p className="text-xs text-destructive">{actionError}</p>}
      </header>
      {groups.length > 0 ? (
        <SessionResultsView
          groups={groups}
          attachmentSrc={(id) => signed[id] ?? ``}
          renderText={(text) => <Markdown source={text} onOpenLink={onOpenLink} />}
          files={files}
          onOpenFile={files.length > 0 ? openFile : undefined}
        />
      ) : (
        <p className="px-7 py-5 text-sm text-muted-foreground md:px-9">
          {live ? `The run has not filed a report yet.` : `The run filed no report.`}
        </p>
      )}
      {hasPr && (
        <RunChanges
          files={files}
          loading={filesLoading}
          error={filesError}
          focusPath={focusPath}
          onMerge={run.prState === `open` ? merge : undefined}
        />
      )}
      <div className="flex-1" />
      {live && <RunSteerComposer key={run.id} runId={run.id} />}
    </article>
  )
}
