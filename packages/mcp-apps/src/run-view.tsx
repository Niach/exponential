import {
  LiveDot,
  Pill,
  SessionResultsView,
  conceptIcon,
  parseSessionResultGroups,
  type LiveDotTone,
} from "@exp/ui"
import { Markdown } from "./markdown"
import { prConcept, runStateLabel, type RunDetail } from "./model"

// EXP-1183 — `exponential_sessions_get` as the run's Results face: what the
// run is about, its state dot (the run rows' tones), its PR, then the REPORT
// through the real `SessionResultsView` (the Guide: Summary first, then the
// numbered topics). Text only: a published picture is an authenticated
// `/api/attachments` URL the sandboxed view cannot load, so the pictures stay
// a count.
export function RunView({
  run,
  onOpenLink,
}: {
  run: RunDetail
  onOpenLink?: (url: string) => void
}) {
  const textEntries = (run.results ?? []).filter(
    (entry) => typeof entry.text === `string` && entry.text.trim().length > 0
  )
  const pictureCount = (run.results ?? []).length - textEntries.length
  const groups = parseSessionResultGroups(textEntries)
  const pr = prConcept(run.prState)
  const PrIcon = pr ? conceptIcon(pr) : null
  const subject = run.issueIdentifier
    ? `${run.issueIdentifier} ${run.issueTitle ?? ``}`.trim()
    : run.actionName || `Chat`
  return (
    <article className="flex flex-col gap-4 py-3">
      <header className="flex flex-col gap-2 px-7">
        <h1 className="text-xl font-semibold leading-tight">{subject}</h1>
        <div className="flex flex-wrap items-center gap-1.5">
          <Pill leading={<LiveDot tone={runTone(run)} ping={isWorking(run)} />}>
            {runStateLabel(run)}
          </Pill>
          {run.agent && <Pill>{run.agent}</Pill>}
          {PrIcon && run.prUrl && (
            <Pill
              mode="action"
              leading={<PrIcon />}
              onClick={() => onOpenLink?.(run.prUrl as string)}
            >
              {run.prNumber != null ? `#${run.prNumber}` : `Pull request`}
            </Pill>
          )}
        </div>
      </header>
      {groups.length > 0 ? (
        <SessionResultsView
          groups={groups}
          attachmentSrc={(id) => id}
          renderText={(text) => (
            <Markdown source={text} onOpenLink={onOpenLink} />
          )}
        />
      ) : (
        <p className="px-7 text-sm text-muted-foreground">
          The run has not filed a report yet.
        </p>
      )}
      {pictureCount > 0 && (
        <p className="px-7 text-xs text-muted-foreground">
          {pictureCount === 1
            ? `1 screenshot is on the run in Exponential.`
            : `${pictureCount} screenshots are on the run in Exponential.`}
        </p>
      )}
    </article>
  )
}

function isWorking(run: RunDetail): boolean {
  return run.status !== `ended` && Boolean(run.agentBusy) && !run.needsInput
}

function runTone(run: RunDetail): LiveDotTone {
  if (run.status === `ended`) return `muted`
  if (run.needsInput) return `attention`
  if (run.status === `in_review`) return `done`
  return run.agentBusy ? `live` : `idle`
}
