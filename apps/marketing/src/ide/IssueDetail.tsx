/* ─── Issue detail center tab: header, Details/Changes, description, activity, 288px properties ─── */
import { useState, type KeyboardEvent } from "react"
import {
  getIssue,
  ISSUE_ACTIVITY,
  ISSUE_BODY,
  PRIORITY_LABEL,
  PROJECT,
  REVIEWS,
  STATUS_LABEL,
  type ActivityItem,
  type Issue,
} from "./data"
import { useIde } from "./state"
import { Avatar, LabelChip, PriorityIcon, StatusIcon } from "./bits"
import { DiffView } from "./Diff"
import {
  IcBell,
  IcBellOff,
  IcBold,
  IcCalDays,
  IcCircleX,
  IcClearFmt,
  IcCode,
  IcGitPullRequest,
  IcH1,
  IcH2,
  IcH3,
  IcImage,
  IcItalic,
  IcLink,
  IcList,
  IcListChecks,
  IcListOrdered,
  IcPlay,
  IcQuote,
  IcSend,
  IcStrike,
  IcTag,
  type IdeIcon,
} from "./icons"

function MdBtn({ Icon, title }: { Icon: IdeIcon; title: string }) {
  return (
    <button className="ide-ghost ide-mdbtn" type="button" title={title}>
      <Icon size={14} />
    </button>
  )
}

const MD_GROUPS: { Icon: IdeIcon; title: string }[][] = [
  [
    { Icon: IcH1, title: `Heading 1` },
    { Icon: IcH2, title: `Heading 2` },
    { Icon: IcH3, title: `Heading 3` },
  ],
  [
    { Icon: IcBold, title: `Bold` },
    { Icon: IcItalic, title: `Italic` },
    { Icon: IcStrike, title: `Strikethrough` },
    { Icon: IcCode, title: `Inline code` },
  ],
  [
    { Icon: IcLink, title: `Link` },
    { Icon: IcQuote, title: `Blockquote` },
  ],
  [
    { Icon: IcList, title: `Bullet list` },
    { Icon: IcListOrdered, title: `Numbered list` },
    { Icon: IcListChecks, title: `Task list` },
  ],
  [{ Icon: IcClearFmt, title: `Clear formatting` }],
  [{ Icon: IcImage, title: `Insert image` }],
]

function MarkdownToolbar() {
  return (
    <div className="ide-mdbar">
      {MD_GROUPS.map((group, gi) => (
        <span key={gi} className="ide-mdgroup">
          {gi > 0 && <span className="ide-mdsep" />}
          {group.map((b) => (
            <MdBtn key={b.title} Icon={b.Icon} title={b.title} />
          ))}
        </span>
      ))}
    </div>
  )
}

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
                <StatusIcon status={getIssue(seg.t).status} size={11} />
                {seg.t}
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
        <span>{item.text}</span>
        <span className="ide-event-time">{`· ${item.time}`}</span>
      </div>
    )
  }
  return (
    <div className="ide-comment">
      <Avatar person={{ initials: item.initials, name: item.author }} size={20} />
      <div className="ide-comment-main">
        <div className="ide-comment-head">
          <span className="ide-comment-author">{item.author}</span>
          <span className="ide-comment-time">{item.time}</span>
        </div>
        <div className="ide-comment-body">{item.body}</div>
      </div>
    </div>
  )
}

function PropGroup({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="ide-prop">
      <span className="ide-prop-label">{label}</span>
      {children}
    </div>
  )
}

function PropsPanel({ issue }: { issue: Issue }) {
  /* One PR per issue — in_review issues carry their open PR. */
  const review = REVIEWS.find((r) => r.issueId === issue.id)
  return (
    <div className="ide-props">
      <PropGroup label="Status">
        <button className="ide-prop-btn" type="button">
          <StatusIcon status={issue.status} />
          {STATUS_LABEL[issue.status]}
        </button>
      </PropGroup>
      <PropGroup label="Priority">
        <button className="ide-prop-btn" type="button">
          <PriorityIcon priority={issue.priority} />
          {PRIORITY_LABEL[issue.priority]}
        </button>
      </PropGroup>
      <PropGroup label="Assignee">
        <button className="ide-prop-btn" type="button">
          <Avatar person={issue.assignee} />
          {issue.assignee ? issue.assignee.name : <span className="ide-c-muted">Unassigned</span>}
        </button>
      </PropGroup>
      <PropGroup label="Labels">
        {issue.labels?.length ? (
          <div className="ide-prop-chips">
            {issue.labels.map((l) => (
              <LabelChip key={l.name} label={l} />
            ))}
          </div>
        ) : (
          <button className="ide-prop-btn ide-c-muted" type="button">
            <IcTag size={14} />
            Add label
          </button>
        )}
      </PropGroup>
      <PropGroup label="Due date">
        <button className="ide-prop-btn" type="button">
          <IcCalDays size={14} className={issue.due ? `ide-c-muted` : `ide-c-dim`} />
          {issue.due ?? <span className="ide-c-muted">Add due date</span>}
        </button>
      </PropGroup>
      {issue.status === `in_review` && review && (
        <PropGroup label="Pull request">
          <button className="ide-prop-btn" type="button">
            <IcGitPullRequest size={14} className="ide-c-green" />
            {`#${review.prNumber} · ${review.branch}`}
          </button>
        </PropGroup>
      )}
      <PropGroup label="Project">
        <span className="ide-prop-chip">
          <span className="ide-proj-dot" style={{ background: PROJECT.color }} />
          {PROJECT.name}
        </span>
      </PropGroup>
    </div>
  )
}

export function IssueDetail({ issueId }: { issueId: string }) {
  const { interactive, coding, codingTarget, codedIssues, requestCoding, stopCoding } = useIde()
  const issue = getIssue(issueId)
  const isExp8 = issue.id === `EXP-8`
  const [detailTab, setDetailTab] = useState<`details` | `changes`>(`details`)
  const [subscribed, setSubscribed] = useState(isExp8)
  const [draft, setDraft] = useState(``)
  const [extraComments, setExtraComments] = useState<ActivityItem[]>([])

  const baseActivity = ISSUE_ACTIVITY[issue.id] ?? []
  const activity = [...baseActivity, ...extraComments]
  /* Coding pill lights for a plain run on this issue AND for a batch run
     that includes it — a batch ships every checked issue. */
  const codingHere =
    coding === `running` &&
    (codingTarget?.kind === `issue`
      ? codingTarget.id === issue.id
      : (codingTarget?.issueIds.includes(issue.id) ?? false))
  /* EXP-8 ships with its diff fixture; other issues earn one by finishing a run. */
  const hasChanges = isExp8 || codedIssues.has(issue.id)

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
      <div className="ide-issue-head">
        <span className="ide-issue-crumb">{issue.id}</span>
        <div className="ide-flex1" />
        {codingHere && (
          <span className="ide-codingpill">
            <span className="ide-codingdot" />
            Coding…
          </span>
        )}
        {codingHere ? (
          <button
            className={`ide-ghost ide-headbtn${interactive ? ` is-click` : ``}`}
            type="button"
            onClick={interactive ? stopCoding : undefined}
          >
            <IcCircleX size={14} className="ide-c-danger" />
            Stop
          </button>
        ) : (
          <button
            className={`ide-ghost ide-headbtn${interactive ? ` is-click` : ``}`}
            type="button"
            onClick={
              interactive ? () => requestCoding({ kind: `issue`, id: issue.id }) : undefined
            }
          >
            <IcPlay size={14} className="ide-c-green" />
            Start coding
          </button>
        )}
        <button
          className={`ide-ghost ide-headbtn${interactive ? ` is-click` : ``}`}
          type="button"
          onClick={interactive ? () => setSubscribed((s) => !s) : undefined}
        >
          {subscribed ? <IcBell size={14} /> : <IcBellOff size={14} />}
          {subscribed ? `Subscribed` : `Subscribe`}
        </button>
      </div>
      <div className="ide-issue-tabs">
        <button
          className={`ide-ghost ide-issue-tab${detailTab === `details` ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
          type="button"
          onClick={interactive ? () => setDetailTab(`details`) : undefined}
        >
          Details
        </button>
        <button
          className={`ide-ghost ide-issue-tab${detailTab === `changes` ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
          type="button"
          onClick={interactive ? () => setDetailTab(`changes`) : undefined}
        >
          Changes
        </button>
      </div>
      {detailTab === `details` ? (
        <div className="ide-issue-body">
          <div className="ide-issue-main">
            <div className="ide-issue-col">
              <div className="ide-issue-title">{issue.title}</div>
              <MarkdownToolbar />
              <Description issueId={issue.id} />
              <div className="ide-attach">
                <span className="ide-attach-count">0 images</span>
              </div>
              <div className="ide-activity">
                <div className="ide-activity-head">{`Activity (${activity.length})`}</div>
                {activity.length === 0 ? (
                  <div className="ide-activity-empty">
                    No activity yet. Be the first to add a comment.
                  </div>
                ) : (
                  activity.map((item, i) => <ActivityRow key={i} item={item} />)
                )}
                <div className="ide-composer">
                  <input
                    className="ide-composer-input"
                    placeholder="Leave a reply..."
                    value={draft}
                    readOnly={!interactive}
                    onChange={(e) => setDraft(e.target.value)}
                    onKeyDown={interactive ? onComposerKey : undefined}
                  />
                  <button
                    className={`ide-send${interactive && draft.trim() ? ` is-click` : ``}`}
                    type="button"
                    disabled={!draft.trim()}
                    onClick={interactive ? submitComment : undefined}
                    title="Send"
                  >
                    <IcSend size={14} />
                  </button>
                </div>
              </div>
            </div>
          </div>
          <PropsPanel issue={issue} />
        </div>
      ) : (
        <div className="ide-issue-changes">
          {hasChanges ? (
            <DiffView />
          ) : (
            <div className="ide-diff-empty">No changes yet. Start coding to open a PR.</div>
          )}
        </div>
      )}
    </div>
  )
}
