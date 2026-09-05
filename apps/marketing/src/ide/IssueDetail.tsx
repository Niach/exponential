/* ─── The issue-detail screen (EXP-417/568/601/723/736/741): a FIXED header
   — switcher row (copy-link · delete right; the Subscribe toggle retired
   with EXP-723) · 2xl title · one glass property tray with Start coding on
   its right edge · the coding-now pill — over a scrolling body that opens
   with the Relations card, then the description, attachments rail and the
   Activity timeline: muted event lines with their time, comment CARDS on
   the timeline rail that each end in a "Leave a reply…" row, and the
   composer. There is no Details/Changes segment and no properties rail. ─── */
import { useState, type KeyboardEvent } from "react"
import {
  getIssue,
  ISSUES,
  ISSUE_ACTIVITY,
  ISSUE_BODY,
  PRIORITY_LABEL,
  REVIEWS,
  STATUS_LABEL,
  type ActivityItem,
  type Issue,
} from "./data"
import { useIde } from "./state"
import { ACTIVE_BOARD } from "./Rail"
import { Avatar, LabelChip, PriorityIcon, StatusIcon } from "./bits"
import {
  IcCalDays,
  IcChevDown,
  IcChevUp,
  IcCircleX,
  IcEllipsis,
  IcGitMerge,
  IcImage,
  IcLink,
  IcLink2,
  IcMonitor,
  IcPaperclip,
  IcPlay,
  IcPlus,
  IcSmile,
  IcTag,
  IcTrash,
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

function PropertyTray({ issue }: { issue: Issue }) {
  const { interactive, coding, codingTarget, requestCoding, stopCoding } = useIde()
  const codingHere =
    coding === `running` &&
    (codingTarget?.kind === `issue`
      ? codingTarget.id === issue.id
      : (codingTarget?.issueIds.includes(issue.id) ?? false))
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
      <span className="ide-tray-action">
        {codingHere ? (
          <button
            className={`ide-btn-outline${interactive ? ` is-click` : ``}`}
            type="button"
            onClick={interactive ? stopCoding : undefined}
          >
            <IcCircleX size={11} className="ide-c-danger" />
            Stop
          </button>
        ) : (
          <button
            className={`ide-btn-primary${interactive ? ` is-click` : ``}`}
            type="button"
            onClick={
              interactive ? () => requestCoding({ kind: `issue`, id: issue.id }) : undefined
            }
          >
            <IcPlay size={11} />
            Start coding
          </button>
        )}
      </span>
    </div>
  )
}

export function IssueDetail({ issueId }: { issueId: string }) {
  const { interactive, coding, codingTarget } = useIde()
  const issue = getIssue(issueId)
  const [armed, setArmed] = useState(false)
  const [draft, setDraft] = useState(``)
  const [extraComments, setExtraComments] = useState<ActivityItem[]>([])

  const baseActivity = ISSUE_ACTIVITY[issue.id] ?? []
  const activity = [...baseActivity, ...extraComments]
  const review = REVIEWS.find((r) => r.issueId === issue.id)
  /* Coding pill lights for a plain run on this issue AND for a batch run
     that includes it — a batch ships every checked issue. */
  const codingHere =
    coding === `running` &&
    (codingTarget?.kind === `issue`
      ? codingTarget.id === issue.id
      : (codingTarget?.issueIds.includes(issue.id) ?? false))
  const position = Math.max(ISSUES.findIndex((i) => i.id === issue.id), 0) + 1

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
          <div className="ide-issue-toprow">
            <span className="ide-switcher">{`${position} / ${ISSUES.length}`}</span>
            <span className="ide-icbtn">
              <IcChevUp size={11} />
            </span>
            <span className="ide-icbtn">
              <IcChevDown size={11} />
            </span>
            <div className="ide-flex1" />
            <span className="ide-icbtn">
              <IcLink size={11} />
            </span>
            <span className="ide-icbtn">
              <IcTrash size={11} />
            </span>
          </div>
          <div className="ide-issue-title">{issue.title}</div>
          <div className="ide-issue-chiprow">
            <PropertyTray issue={issue} />
          </div>
          {/* issue_header::agent_row (EXP-698): the coding-now CARD on its own
              full-width line, and the Merge-PR capsule as a TRAILING action
              INSIDE it whenever both show — one tray holds the run and
              everything to do about it. With no live session the merge
              control falls back to its own row. */}
          {codingHere ? (
            <div className="ide-issue-agentrow">
              <span className="ide-nowpill">
                <span className="ide-nowdot" />
                Coding now
              </span>
              <span className="ide-nowcaption">
                Danny Strähhuber · Danny&apos;s MacBook Pro
              </span>
              <div className="ide-flex1" />
              {/* An own run gets the primary "Watch" pill — NAV_DEVICES
                  (monitor), never an eye. */}
              <button className="ide-btn-primary ide-nowwatch" type="button">
                <IcMonitor size={11} />
                Watch
              </button>
              {review && <MergePrButton armed={armed} arm={setArmed} />}
            </div>
          ) : review ? (
            <div className="ide-issue-agentrow is-bare">
              <MergePrButton armed={armed} arm={setArmed} />
            </div>
          ) : null}
        </div>
      </div>
      <div className="ide-issue-body">
        {/* issue_relations::render_relations_card (EXP-736) opens the body:
            the header + "Add relation" chip stand alone above an empty list. */}
        <div className="ide-col ide-relations">
          <div className="ide-relcard">
            <IcLink2 size={10.5} />
            <span className="ide-relcard-label">Relations</span>
            <span className="ide-tchip">
              <IcPlus size={10.5} />
              Add relation
            </span>
          </div>
        </div>
        <div className="ide-col">
          <Description issueId={issue.id} />
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
