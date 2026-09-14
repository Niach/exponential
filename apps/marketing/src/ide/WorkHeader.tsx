/* ─── The ONE work header (work_header.rs, EXP-877) — shared by the Issue,
   Run and Diff faces of a top tab, so the title never moves between them.
   Row 1: the 24/32 semibold title with the right cluster on the same line:
   the `Issue | Run | +N −M` face toggle (hidden below two items), then, for
   an issue, its pin and `…` menu. Row 2 (issue-bound only): the glass
   property tray, trailing `[Merge PR] [the ONE coding action]` — Stop for
   my live run, else Start coding. An issue-less run (a batch) has no tray;
   its Merge PR and Stop ride the right cluster instead. ─── */
import { useState, type ReactNode } from "react"
import {
  BATCH_RUN_TITLE,
  getIssue,
  PRIORITY_LABEL,
  REVIEWS,
  STATUS_LABEL,
  type Issue,
} from "./data"
import { isLive, useIde, type Face, type RunView } from "./state"
import { ACTIVE_BOARD } from "./Rail"
import { LabelChip, PriorityIcon, StatusIcon } from "./bits"
import {
  IcCalDays,
  IcCircleUser,
  IcCircleX,
  IcEllipsis,
  IcGitMerge,
  IcPin,
  IcPlay,
  IcTag,
} from "./icons"

function Chip({ children, muted }: { children: ReactNode; muted?: boolean }) {
  return <span className={`ide-tchip${muted ? ` is-muted` : ``}`}>{children}</span>
}

/* pr_merge::two_click — Merge PR arms, Confirm merge fires (danger). */
function MergePrButton({ issueId, primary }: { issueId: string; primary: boolean }) {
  const { interactive, mergeReview, mergedReviews } = useIde()
  const [armed, setArmed] = useState(false)
  const merged = mergedReviews.has(issueId)
  return (
    <button
      className={`ide-pill${primary ? ` is-primary` : ``}${armed ? ` is-armed` : ``}${interactive && !merged ? ` is-click` : ``}`}
      type="button"
      disabled={merged}
      onClick={
        interactive && !merged
          ? () => {
              if (armed) {
                setArmed(false)
                mergeReview(issueId)
              } else {
                setArmed(true)
              }
            }
          : undefined
      }
    >
      <IcGitMerge size={11} />
      {merged ? `Merged` : armed ? `Confirm merge` : `Merge PR`}
    </button>
  )
}

function StopButton({ run }: { run: RunView }) {
  const { interactive, stopRun } = useIde()
  return (
    <button
      className={`ide-pill${interactive ? ` is-click` : ``}`}
      type="button"
      title="Stop the agent and end the session"
      onClick={interactive ? () => stopRun(run.id) : undefined}
    >
      <IcCircleX size={11} className="ide-c-danger" />
      Stop
    </button>
  )
}

function PropertyTray({ issue, run }: { issue: Issue; run: RunView | null }) {
  const { interactive, openComposer, goneReviews } = useIde()
  const review = REVIEWS.find((r) => r.issueId === issue.id && !goneReviews.has(r.issueId))
  const live = run && isLive(run.state)
  return (
    <div className="ide-tray">
      <Chip>
        <StatusIcon status={issue.status} size={11} />
        {STATUS_LABEL[issue.status]}
      </Chip>
      <Chip>
        <PriorityIcon priority={issue.priority} size={11} />
        {PRIORITY_LABEL[issue.priority]}
      </Chip>
      <Chip muted={!issue.assignee}>
        <IcCircleUser size={11} />
        {issue.assignee ? issue.assignee.name : `Assignee`}
      </Chip>
      {issue.labels?.length ? (
        issue.labels.map((l) => <LabelChip key={l.name} label={l} />)
      ) : (
        <Chip muted>
          <IcTag size={11} />
          Labels
        </Chip>
      )}
      <Chip muted={!issue.due}>
        <IcCalDays size={11} />
        {issue.due ?? `Due date`}
      </Chip>
      <Chip>
        <ACTIVE_BOARD.Icon size={11} style={{ color: ACTIVE_BOARD.color }} />
        {ACTIVE_BOARD.name}
      </Chip>
      <span className="ide-tray-action">
        {review && <MergePrButton issueId={issue.id} primary={!live} />}
        {live ? (
          <StopButton run={run} />
        ) : (
          <button
            className={`ide-pill${review ? `` : ` is-primary`}${interactive ? ` is-click` : ``}`}
            type="button"
            /* EXP-825: a play button NAVIGATES to the Agent page with this
               issue chipped. */
            onClick={interactive ? () => openComposer([issue.id]) : undefined}
          >
            <IcPlay size={11} />
            Start coding
          </button>
        )}
      </span>
    </div>
  )
}

/* The `Issue | Run | +N −M` segmented capsule. */
function FaceToggle({ tabKey, hasIssue, run }: { tabKey: string; hasIssue: boolean; run: RunView | null }) {
  const { faceOf, setFace, interactive } = useIde()
  const face = faceOf(tabKey)
  const diff = run
    ? run.files.reduce((acc, f) => ({ add: acc.add + f.add, del: acc.del + f.del }), { add: 0, del: 0 })
    : null
  const edited =
    run !== null &&
    run.rows.slice(0, run.pos.done).some((row) => row.kind === `tool` && row.verb === `Edit`)
  const items: Face[] = []
  if (hasIssue) items.push(`issue`)
  if (run) {
    items.push(`run`)
    if (edited) items.push(`diff`)
  }
  if (items.length < 2) return null
  const current: Face = items.includes(face) ? face : items[0]
  return (
    <div className="ide-seg">
      {items.map((item) => (
        <button
          key={item}
          className={`ide-seg-item${item === current ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
          type="button"
          onClick={interactive && item !== current ? () => setFace(tabKey, item) : undefined}
        >
          {item === `issue` ? (
            `Issue`
          ) : item === `run` ? (
            `Run`
          ) : (
            <span className="ide-seg-diff">
              <span className="ide-c-green">{`+${diff?.add ?? 0}`}</span>
              <span className="ide-c-red">{`-${diff?.del ?? 0}`}</span>
            </span>
          )}
        </button>
      ))}
    </div>
  )
}

export function WorkHeader({ tabKey }: { tabKey: string }) {
  const { tabs, runForTab, goneReviews } = useIde()
  const tab = tabs.find((t) => t.key === tabKey)
  const run = runForTab(tabKey)
  const issue = tab?.kind === `issue` ? getIssue(tab.ref) : null
  const title = issue ? issue.title : (run?.title ?? BATCH_RUN_TITLE)
  const batchReview =
    !issue && run && run.state === `review` && !goneReviews.has(`batch`)
  return (
    <div className="ide-workhead">
      <div className="ide-workcol">
        <div className="ide-workhead-row">
          <div className="ide-worktitle">{title}</div>
          <div className="ide-workhead-right">
            <FaceToggle tabKey={tabKey} hasIssue={issue !== null} run={run} />
            {issue ? (
              <>
                <span className="ide-headicon" title="Pin">
                  <IcPin size={13} />
                </span>
                <span className="ide-headicon" title="More">
                  <IcEllipsis size={14} />
                </span>
              </>
            ) : (
              run && (
                <>
                  {batchReview && <MergePrButton issueId="batch" primary={false} />}
                  {isLive(run.state) && <StopButton run={run} />}
                </>
              )
            )}
          </div>
        </div>
        {issue && <PropertyTray issue={issue} run={run} />}
      </div>
    </div>
  )
}
