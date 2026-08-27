/* ─── The issue-detail screen (EXP-417/568/601): a FIXED header — switcher
   row · 2xl title · one glass property tray with Start coding on its right
   edge · the coding-now pill — over a scrolling body of description,
   attachments rail and the Activity timeline with its reply composer.
   There is no Details/Changes segment and no right-hand properties rail. ─── */
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
  IcBell,
  IcBellOff,
  IcCalDays,
  IcChevDown,
  IcChevUp,
  IcCircleX,
  IcEllipsis,
  IcGitPullRequest,
  IcImage,
  IcLink,
  IcPaperclip,
  IcPlay,
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

function ActivityRow({ item }: { item: ActivityItem }) {
  if (item.kind === `event`) {
    return (
      <div className="ide-event">
        <span className="ide-event-dot" />
        <span className="ide-event-text">{item.text}</span>
        <span className="ide-event-time">{`· ${item.time}`}</span>
      </div>
    )
  }
  return (
    <div className="ide-comment">
      <Avatar person={{ initials: item.initials, name: item.author }} size={18} />
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
      </div>
    </div>
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
  const [subscribed, setSubscribed] = useState(true)
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
            <button
              className={`ide-icbtn${interactive ? ` is-click` : ``}`}
              type="button"
              title={subscribed ? `Unsubscribe` : `Subscribe`}
              onClick={interactive ? () => setSubscribed((s) => !s) : undefined}
            >
              {subscribed ? <IcBell size={11} /> : <IcBellOff size={11} />}
            </button>
            <span className="ide-icbtn">
              <IcTrash size={11} />
            </span>
          </div>
          <div className="ide-issue-title">{issue.title}</div>
          <div className="ide-issue-chiprow">
            <PropertyTray issue={issue} />
          </div>
          {codingHere && (
            <div className="ide-issue-agentrow">
              <span className="ide-nowpill">
                <span className="ide-nowdot" />
                Danny Strähhuber coding now · Danny&apos;s MacBook Pro
              </span>
            </div>
          )}
        </div>
      </div>
      <div className="ide-issue-body">
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
          {review && (
            <div className="ide-prrow">
              <IcGitPullRequest size={11} className="ide-c-green" />
              <span className="ide-prrow-state">Open</span>
              <span className="ide-prrow-num">{`#${review.prNumber}`}</span>
              <span className="ide-prrow-branch">{review.branch}</span>
            </div>
          )}
        </div>
        <div className="ide-timeline">
          <div className="ide-col">
            <div className="ide-activity-head">{`Activity (${activity.length})`}</div>
            {activity.map((item, i) => (
              <ActivityRow key={i} item={item} />
            ))}
            <div className="ide-composer">
              <input
                className="ide-composer-input"
                placeholder="Leave a reply..."
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
