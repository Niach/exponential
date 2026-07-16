/* ─── Full-page web issue detail ───
   Mirrors apps/web issue-detail-view.tsx: breadcrumb row, Details·Changes
   segmented control, centered title + GFM description, activity timeline
   with composer, 288px properties rail (no release row, no Start coding —
   coding runs are desktop-only; a slim "Coding now" strip stands in for the
   steer panel). */
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
} from "../ide/data"
import { useWeb } from "./state"
import { Avatar, LabelChip, PriorityIcon, StatusIcon } from "../ide/bits"
import { DiffView } from "../ide/Diff"
import {
  IcBell,
  IcBellOff,
  IcCalDays,
  IcChevRight,
  IcSend,
  IcTag,
} from "../ide/icons"
import { IcLink2, IcMore } from "./icons"
import { WEB_USER } from "./data"

function Description({ issueId }: { issueId: string }) {
  const body = ISSUE_BODY[issueId]
  if (!body) {
    return <div className="web-desc is-empty">Add description...</div>
  }
  return (
    <div className="web-desc">
      {body.map((para, pi) => (
        <p key={pi}>
          {para.map((seg, si) =>
            seg.code ? (
              <code key={si} className="ide-inlinecode">
                {seg.t}
              </code>
            ) : seg.ref ? (
              /* #issue mention — plain `#EXP-5` in the markdown source,
                 rendered as a pill when it resolves in-workspace */
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
      <div className="web-event">
        <span className="web-event-dot" />
        <span>{item.text}</span>
        <span className="ide-c-dim">{`· ${item.time}`}</span>
      </div>
    )
  }
  return (
    <div className="web-comment">
      <Avatar person={{ initials: item.initials, name: item.author }} size={22} />
      <div className="web-comment-main">
        <div className="web-comment-head">
          <span className="web-comment-author">{item.author}</span>
          <span className="web-comment-time">{item.time}</span>
        </div>
        <div className="web-comment-body">{item.body}</div>
      </div>
    </div>
  )
}

function PropGroup({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="web-prop">
      <span className="web-prop-label">{label}</span>
      {children}
    </div>
  )
}

function PropsPanel({ issue }: { issue: Issue }) {
  return (
    <aside className="web-props">
      <PropGroup label="Status">
        <button className="web-prop-btn" type="button">
          <StatusIcon status={issue.status} size={13} />
          {STATUS_LABEL[issue.status]}
        </button>
      </PropGroup>
      <PropGroup label="Priority">
        <button className="web-prop-btn" type="button">
          <PriorityIcon priority={issue.priority} size={13} />
          {PRIORITY_LABEL[issue.priority]}
        </button>
      </PropGroup>
      <PropGroup label="Assignee">
        <button className="web-prop-btn" type="button">
          <Avatar person={issue.assignee} size={16} />
          {issue.assignee ? issue.assignee.name : <span className="ide-c-muted">Unassigned</span>}
        </button>
      </PropGroup>
      <PropGroup label="Labels">
        {issue.labels?.length ? (
          <div className="web-prop-chips">
            {issue.labels.map((l) => (
              <LabelChip key={l.name} label={l} />
            ))}
          </div>
        ) : (
          <button className="web-prop-btn ide-c-muted" type="button">
            <IcTag size={13} />
            Add label
          </button>
        )}
      </PropGroup>
      <PropGroup label="Due date">
        <button className="web-prop-btn" type="button">
          <IcCalDays size={13} className={issue.due ? `ide-c-muted` : `ide-c-dim`} />
          {issue.due ?? <span className="ide-c-muted">Due date</span>}
        </button>
      </PropGroup>
      <PropGroup label="Project">
        <span className="web-prop-chip">
          <span className="web-proj-dot" style={{ background: PROJECT.color }} />
          {PROJECT.name}
        </span>
      </PropGroup>
    </aside>
  )
}

export function WebIssueDetail({ issueId }: { issueId: string }) {
  const { interactive, closeIssue } = useWeb()
  const issue = getIssue(issueId)
  const isExp8 = issue.id === `EXP-8`
  const hasPr = REVIEWS.some((r) => r.issueId === issue.id)
  const [tab, setTab] = useState<`details` | `changes`>(`details`)
  const [subscribed, setSubscribed] = useState(isExp8)
  const [draft, setDraft] = useState(``)
  const [extraComments, setExtraComments] = useState<ActivityItem[]>([])

  const activity = [...(ISSUE_ACTIVITY[issue.id] ?? []), ...extraComments]

  const submitComment = () => {
    const body = draft.trim()
    if (!body) return
    setExtraComments((prev) => [
      ...prev,
      {
        kind: `comment`,
        author: WEB_USER.name,
        initials: WEB_USER.initials,
        time: `just now`,
        body,
      },
    ])
    setDraft(``)
  }

  const onComposerKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === `Enter` && !e.shiftKey) {
      e.preventDefault()
      submitComment()
    }
  }

  return (
    <div className="web-detail">
      <div className="web-crumbs">
        <button
          className={`web-crumb-proj${interactive ? ` is-click` : ``}`}
          type="button"
          onClick={interactive ? closeIssue : undefined}
        >
          <span className="web-proj-dot" style={{ background: PROJECT.color }} />
          {PROJECT.name}
        </button>
        <IcChevRight size={12} className="ide-c-dim" />
        <span className="web-crumb-id">{issue.id}</span>
        <IcChevRight size={12} className="ide-c-dim" />
        <span className="web-crumb-title">{issue.title}</span>
        <div className="web-crumb-actions">
          <button className="web-ghost web-icbtn" type="button" title="Copy link to issue">
            <IcLink2 size={14} />
          </button>
          <button
            className={`web-ghost web-icbtn${interactive ? ` is-click` : ``}`}
            type="button"
            title={subscribed ? `Subscribed` : `Subscribe`}
            onClick={interactive ? () => setSubscribed((s) => !s) : undefined}
          >
            {subscribed ? <IcBell size={14} /> : <IcBellOff size={14} />}
          </button>
          <button className="web-ghost web-icbtn" type="button" title="Issue actions">
            <IcMore size={14} />
          </button>
        </div>
      </div>

      <div className="web-detail-body">
        <div className="web-detail-main">
          <div className="web-segbar">
            <div className="web-seg">
              <button
                className={`web-seg-btn${tab === `details` ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
                type="button"
                onClick={interactive ? () => setTab(`details`) : undefined}
              >
                Details
              </button>
              <button
                className={`web-seg-btn${tab === `changes` ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
                type="button"
                onClick={interactive ? () => setTab(`changes`) : undefined}
              >
                Changes
                {hasPr && <span className="web-seg-dot" />}
              </button>
            </div>
          </div>

          {tab === `details` ? (
            <div className="web-detail-scroll">
              <div className="web-detail-col">
                <div className="web-issue-title">{issue.title}</div>
                <Description issueId={issue.id} />
                {isExp8 && (
                  <div className="web-coding">
                    <span className="web-coding-dot" />
                    {`Coding now · Claude on Danny's desktop`}
                  </div>
                )}
                <div className="web-activity">
                  <div className="web-activity-head">{`Activity (${activity.length})`}</div>
                  {activity.length === 0 ? (
                    <div className="web-activity-empty">
                      No activity yet. Be the first to add a comment.
                    </div>
                  ) : (
                    activity.map((item, i) => <ActivityRow key={i} item={item} />)
                  )}
                  <div className="web-composer">
                    <textarea
                      className="web-composer-input"
                      placeholder="Leave a reply…"
                      rows={2}
                      value={draft}
                      readOnly={!interactive}
                      onChange={(e) => setDraft(e.target.value)}
                      onKeyDown={interactive ? onComposerKey : undefined}
                    />
                    <button
                      className={`web-send${interactive && draft.trim() ? ` is-click` : ``}`}
                      type="button"
                      disabled={!draft.trim()}
                      onClick={interactive ? submitComment : undefined}
                      title="Send comment"
                    >
                      <IcSend size={14} />
                    </button>
                  </div>
                </div>
              </div>
            </div>
          ) : (
            <div className="web-detail-changes">
              {hasPr ? (
                <DiffView />
              ) : (
                <div className="web-changes-empty">
                  No changes yet. Coding sessions run from the desktop IDE.
                </div>
              )}
            </div>
          )}
        </div>
        <PropsPanel issue={issue} />
      </div>
    </div>
  )
}
