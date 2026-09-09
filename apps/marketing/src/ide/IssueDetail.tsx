/* ─── The issue-detail screen (EXP-417/568/601/723/736/741/760): a FIXED
   header — the top row, which is the round "…" actions menu and nothing else
   (EXP-760 folded Copy link and Delete into it; the Subscribe toggle retired
   with EXP-723) · 2xl title · one glass property tray carrying BOTH header
   actions on its right edge, Start coding and, while the PR is open, Merge ·
   the agent row — over a scrolling body: the description, the relation
   groups (EXP-760 moved them BELOW it and drops them entirely when there are
   none), "Add sub-issues", the attachments rail and the Activity timeline —
   muted event lines with their time, comment CARDS on the timeline rail that
   each end in a "Leave a reply…" row, and the composer. There is no
   Details/Changes segment and no properties rail. ─── */
import { useState, type KeyboardEvent } from "react"
import {
  getIssue,
  ISSUE_ACTIVITY,
  ISSUE_BODY,
  PRIORITY_LABEL,
  REVIEWS,
  STATUS_LABEL,
  type ActivityItem,
  type Issue,
  type Review,
} from "./data"
import { useIde, type CodingState, type CodingTarget } from "./state"
import { ACTIVE_BOARD } from "./Rail"
import { Avatar, LabelChip, PriorityIcon, StatusIcon } from "./bits"
import {
  IcCalDays,
  IcCircleX,
  IcEllipsis,
  IcGitMerge,
  IcImage,
  IcMonitor,
  IcPaperclip,
  IcPlay,
  IcPlus,
  IcSmile,
  IcTag,
  IcCircleUser,
} from "./icons"

function Description({ issueId }: { issueId: string }) {
  const body = ISSUE_BODY[issueId]
  if (!body) {
    return <div className="ide-issue-desc is-empty">Add description...</div>
  }
  return (
    <div className="ide-issue-desc">
      {body.map((para, pi) => (
        <p key={pi}>
          {para.map((seg, si) =>
            seg.code ? (
              <code key={si} className="ide-inlinecode">
                {seg.t}
              </code>
            ) : seg.ref ? (
              /* #issue mention — plain `#EXP-5` in the markdown source,
                 rendered as a clickable pill when it resolves in-workspace */
              <span key={si} className="ide-refpill">
                <StatusIcon status={getIssue(seg.t).status} size={9} />
                {`#${seg.t}`}
              </span>
            ) : seg.mention ? (
              /* @mention — `@<email>` in the source, name pill at render */
              <span key={si} className="ide-mentionpill">{`@${seg.t}`}</span>
            ) : (
              <span key={si}>{seg.t}</span>
            ),
          )}
        </p>
      ))}
    </div>
  )
}

/* timeline::timeline_row — every feed row rides the 28px gutter; the
   marker's box (--mk-top/--mk-size) tells the rail where to break. */
function TimelineRow({
  marker,
  markerTop,
  markerSize,
  pad,
  first,
  last,
  children,
}: {
  marker: React.ReactNode
  markerTop: number
  markerSize: number
  pad?: number
  first?: boolean
  last?: boolean
  children: React.ReactNode
}) {
  return (
    <div
      className={`ide-tl${first ? ` is-first` : ``}${last ? ` is-last` : ``}`}
      style={
        {
          "--mk-top": `${markerTop}px`,
          "--mk-size": `${markerSize}px`,
          ...(pad !== undefined ? { "--tl-pad": `${pad}px` } : {}),
        } as React.CSSProperties
      }
    >
      <div className="ide-tl-gutter">
        <div className="ide-tl-marker">{marker}</div>
      </div>
      <div className="ide-tl-body">{children}</div>
    </div>
  )
}

/* Events are "<actor> <phrase>" in the fixture; the actor leads in the
   foreground, the phrase and the time stay muted (timeline.rs event_row). */
const EVENT_ACTOR = /^(.+?) (changed|added|removed|assigned|opened|merged|moved|created) (.*)$/

function ActivityRow({
  item,
  first,
  last,
}: {
  item: ActivityItem
  first?: boolean
  last?: boolean
}) {
  if (item.kind === `event`) {
    const m = EVENT_ACTOR.exec(item.text)
    return (
      <TimelineRow
        marker={<span className="ide-event-dot" />}
        markerTop={7}
        markerSize={6}
        first={first}
        last={last}
      >
        <div className="ide-event">
          {m ? (
            <>
              <span className="ide-event-actor">{m[1]}</span>
              <span className="ide-event-text">{`${m[2]} ${m[3]} · ${item.time}`}</span>
            </>
          ) : (
            <span className="ide-event-text">{`${item.text} · ${item.time}`}</span>
          )}
        </div>
      </TimelineRow>
    )
  }
  return (
    <TimelineRow
      marker={<Avatar person={{ initials: item.initials, name: item.author }} size={24} />}
      markerTop={4}
      markerSize={24}
      pad={0}
      first={first}
      last={last}
    >
      <div className="ide-comment">
        <div className="ide-comment-main">
          <div className="ide-comment-head">
            <span className="ide-comment-author">{item.author}</span>
            <span className="ide-comment-time">{item.time}</span>
            <div className="ide-flex1" />
            <span className="ide-comment-menu">
              <IcEllipsis size={11} />
            </span>
          </div>
          <div className="ide-comment-body">{item.body}</div>
          {/* EXP-741: the card is the thread — the reply row closes it. */}
          <div className="ide-comment-replies">
            <button className="ide-reply-row" type="button">
              Leave a reply…
            </button>
          </div>
        </div>
      </div>
    </TimelineRow>
  )
}

/* A live run on this issue — a plain run on it, or a batch that includes
   it (a batch ships every checked issue). */
function isCodingHere(
  coding: CodingState,
  target: CodingTarget | null,
  issueId: string,
): boolean {
  if (coding !== `running` && coding !== `waiting`) return false
  return target?.kind === `issue`
    ? target.id === issueId
    : (target?.issueIds.includes(issueId) ?? false)
}

/* pr_merge::two_click — Merge PR arms, Confirm merge fires (danger). */
function MergePrButton({
  armed,
  arm,
}: {
  armed: boolean
  arm: (on: boolean) => void
}) {
  const { interactive } = useIde()
  return (
    <button
      className={`ide-mergepr${armed ? ` is-armed` : ``}${interactive ? ` is-click` : ``}`}
      type="button"
      onClick={interactive ? () => arm(!armed) : undefined}
    >
      <IcGitMerge size={11} />
      {armed ? `Confirm merge` : `Merge PR`}
    </button>
  )
}

/* One chip of the glass property tray: a ghost h-24 capsule. */
function Chip({
  children,
  muted,
}: {
  children: React.ReactNode
  muted?: boolean
}) {
  return <span className={`ide-tchip${muted ? ` is-muted` : ``}`}>{children}</span>
}

function PropertyTray({
  issue,
  review,
  armed,
  arm,
}: {
  issue: Issue
  /* An open PR on this issue — the tray then carries Merge as well. */
  review?: Review
  armed: boolean
  arm: (on: boolean) => void
}) {
  const { interactive, coding, codingTarget, requestCoding, stopCoding } = useIde()
  const codingHere = isCodingHere(coding, codingTarget, issue.id)
  return (
    <div className="ide-tray">
      <Chip>
        <StatusIcon status={issue.status} size={10} />
        {STATUS_LABEL[issue.status]}
      </Chip>
      <Chip>
        <PriorityIcon priority={issue.priority} size={10} />
        {PRIORITY_LABEL[issue.priority]}
      </Chip>
      <Chip muted={!issue.assignee}>
        <IcCircleUser size={10} />
        {issue.assignee ? issue.assignee.name : `Assignee`}
      </Chip>
      {issue.labels?.length ? (
        issue.labels.map((l) => <LabelChip key={l.name} label={l} />)
      ) : (
        <Chip muted>
          <IcTag size={10} />
          Labels
        </Chip>
      )}
      <Chip muted={!issue.due}>
        <IcCalDays size={10} />
        {issue.due ?? `Due date`}
      </Chip>
      <Chip>
        <ACTIVE_BOARD.Icon size={10} style={{ color: ACTIVE_BOARD.color }} />
        {ACTIVE_BOARD.name}
      </Chip>
      {/* EXP-760: BOTH header actions live at the tray's right edge — Start
          coding and, while the PR is open, Merge (which takes the white
          paint and demotes Start coding to plain glass). A run THIS machine
          hosts replaces the launcher with the "Coding…" indicator and Stop
          (coding_flow.rs), since that control is the only way to stop it. */}
      <span className="ide-tray-action">
        {codingHere ? (
          <>
            <span className="ide-codingnow">
              <span className="ide-nowdot" />
              Coding…
            </span>
            <button
              className={`ide-btn-outline${interactive ? ` is-click` : ``}`}
              type="button"
              onClick={interactive ? stopCoding : undefined}
            >
              <IcCircleX size={11} className="ide-c-danger" />
              Stop
            </button>
          </>
        ) : (
          <button
            className={`${review ? `ide-btn-glass` : `ide-btn-primary`}${interactive ? ` is-click` : ``}`}
            type="button"
            onClick={
              interactive ? () => requestCoding({ kind: `issue`, id: issue.id }) : undefined
            }
          >
            <IcPlay size={11} />
            Start coding
          </button>
        )}
        {review && <MergePrButton armed={armed} arm={arm} />}
      </span>
    </div>
  )
}

export function IssueDetail({ issueId }: { issueId: string }) {
  const { interactive, coding, codingTarget, openSession } = useIde()
  const issue = getIssue(issueId)
  const [armed, setArmed] = useState(false)
  const [draft, setDraft] = useState(``)
  const [extraComments, setExtraComments] = useState<ActivityItem[]>([])

  const baseActivity = ISSUE_ACTIVITY[issue.id] ?? []
  const activity = [...baseActivity, ...extraComments]
  const review = REVIEWS.find((r) => r.issueId === issue.id)
  const codingHere = isCodingHere(coding, codingTarget, issue.id)
  /* The fixture's OTHER live run — a teammate's machine, which is what puts
     the coding-now card and its Watch on an issue. */
  const otherMachineRun = issue.id === `EXP-11`

  const submitComment = () => {
    const body = draft.trim()
    if (!body) return
    setExtraComments((prev) => [
      ...prev,
      { kind: `comment`, author: `Danny Strähhuber`, initials: `DS`, time: `just now`, body },
    ])
    setDraft(``)
  }

  const onComposerKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === `Enter`) submitComment()
  }

  return (
    <div className="ide-issue">
      <div className="ide-issue-header">
        <div className="ide-col">
          {/* issue_header::top_row — ONE round menu, right-aligned: Copy
              link · Add relation ▸ · Delete (EXP-760). */}
          <div className="ide-issue-toprow">
            <div className="ide-flex1" />
            <span className="ide-roundbtn">
              <IcEllipsis size={11} />
            </span>
          </div>
          <div className="ide-issue-title">{issue.title}</div>
          <div className="ide-issue-chiprow">
            <PropertyTray issue={issue} review={review} armed={armed} arm={setArmed} />
          </div>
          {/* issue_header::agent_row: the coding-now CARD, for a run on
              ANOTHER machine — this machine's own run is the tray's
              "Coding…"/Stop control instead, and the card would say the same
              thing twice. Watch slides the run in over the issue (EXP-791);
              Merge left this row for the tray with EXP-760. */}
          {codingHere ? null : otherMachineRun ? (
            <div className="ide-issue-agentrow">
              <span className="ide-nowpill">
                <span className="ide-nowdot" />
                Coding now
              </span>
              <span className="ide-nowcaption">
                Mira Chen · Mira&apos;s MacBook Pro
              </span>
              <div className="ide-flex1" />
              {/* An own run gets the primary "Watch" pill — NAV_DEVICES
                  (monitor), never an eye. */}
              <button
                className={`ide-btn-primary ide-nowwatch${interactive ? ` is-click` : ``}`}
                type="button"
                onClick={interactive ? openSession : undefined}
              >
                <IcMonitor size={11} />
                Watch
              </button>
            </div>
          ) : null}
        </div>
      </div>
      <div className="ide-issue-body">
        {/* EXP-760: relations are Linear-style group headings BELOW the
            description ("Sub-issues 1/3", "Blocked by", …) and render
            nothing at all when the issue has none — which is this fixture.
            "Add sub-issues" follows them and opens the inline composer. */}
        <div className="ide-col">
          <Description issueId={issue.id} />
          <button className="ide-addsub" type="button">
            <IcPlus size={10.5} />
            Add sub-issues
          </button>
          <div className="ide-attachrail">
            <span className="ide-icbtn">
              <IcSmile size={12} />
            </span>
            <span className="ide-icbtn">
              <IcImage size={12} />
            </span>
            <span className="ide-icbtn">
              <IcPaperclip size={12} />
            </span>
          </div>
        </div>
        <div className="ide-timeline">
          <div className="ide-col">
            <div className="ide-activity-head">{`Activity (${activity.length})`}</div>
            {activity.map((item, i) => (
              <ActivityRow
                key={i}
                item={item}
                first={i === 0}
                last={i === activity.length - 1}
              />
            ))}
            <div className="ide-composer">
              <input
                className="ide-composer-input"
                placeholder="Leave a reply…"
                value={draft}
                readOnly={!interactive}
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={interactive ? onComposerKey : undefined}
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
