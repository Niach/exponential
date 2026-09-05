/* ─── Full-page web issue detail ───
   Mirrors apps/web issue-detail-view.tsx after EXP-568/723/736/741:
   breadcrumb bar with the position switcher / copy-link / delete actions
   (the Subscribe toggle is gone since EXP-723), then ONE centered max-w-3xl
   reading column — big title, the properties GLASS CARD (no sidebar rail any
   more), the "Coding now" glass row, the Relations card, the markdown
   description with the editor's insert rail, and the activity timeline
   (issue-timeline.tsx + comment-rows/*) whose comment cards end in the
   "Leave a reply…" row. */
import { useState } from "react"
import {
  getIssue,
  ISSUES,
  ISSUE_ACTIVITY,
  ISSUE_BODY,
  PRIORITY_LABEL,
  STATUS_LABEL,
  type ActivityItem,
  type Issue,
  type IssueStatus,
} from "../ide/data"
import { useWeb } from "./state"
import { StatusGlyph, PriorityGlyph, WebAvatar } from "./bits"
import { AGENT_SESSIONS, WEB_BOARD, WEB_USER } from "./data"
import { WebAgentDock } from "./Board"
import {
  ICON_3,
  ICON_35,
  ICON_4,
  IcCalendar,
  IcChevDown,
  IcChevRight,
  IcChevUp,
  IcCode,
  IcDevices,
  IcHash,
  IcImage,
  IcLink2,
  IcPaperclip,
  IcPlus,
  IcSmile,
  IcSubmit,
  IcTag,
  IcTrash,
} from "./icons"

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
              <code key={si} className="web-code">
                {seg.t}
              </code>
            ) : seg.ref ? (
              /* #issue reference — plain `#EXP-5` in the markdown source; the
                 pill only renders when the token resolves to a synced
                 same-team issue, and carries that issue's title (EXP-307). */
              <span key={si} className="web-refpill">
                <StatusGlyph status={getIssue(seg.t).status} size={ICON_3} />
                {`#${seg.t}`}
                <span className="web-refpill-title">{getIssue(seg.t).title}</span>
              </span>
            ) : seg.mention ? (
              /* @mention — `@<email>` in the source, name pill at render */
              <span key={si} className="web-mentionpill">{`@${seg.t}`}</span>
            ) : (
              <span key={si}>{seg.t}</span>
            ),
          )}
        </p>
      ))}
    </div>
  )
}

/* Status-change events draw the target status's glyph in the icon slot
   (comment-rows/event.tsx); everything else gets the plain dot. The actor
   name and the changed value are the only foreground words in the line. */
const STATUS_BY_LABEL: Record<string, IssueStatus> = {
  Backlog: `backlog`,
  [`In Progress`]: `in_progress`,
  [`In Review`]: `in_review`,
  Done: `done`,
}

function EventRow({ actor, text, value, time }: {
  actor: string
  text: string
  value?: string
  /* EXP-723: every event line ends in its own relative time
     (comment-rows/event.tsx), the way the creation row always did. */
  time?: string
}) {
  const status = value ? STATUS_BY_LABEL[value] : undefined
  return (
    <div className="web-event">
      <span className="web-event-icon">
        {status ? (
          <StatusGlyph status={status} size={ICON_35} />
        ) : (
          <span className="web-event-dot" />
        )}
      </span>
      <span className="web-event-text">
        <b>{actor}</b> {text}
        {value && <b>{value}</b>}
        {time && ` · ${time}`}
      </span>
    </div>
  )
}

function ActivityRow({ item }: { item: ActivityItem }) {
  if (item.kind === `event`) {
    /* The fixture text is one sentence: "<actor> changed status to <value>". */
    const m = /^(.+?) (changed status to )(.+)$/.exec(item.text)
    return m ? (
      <EventRow actor={m[1]} text={m[2]} value={m[3]} time={item.time} />
    ) : (
      <EventRow actor={``} text={item.text} time={item.time} />
    )
  }
  return (
    <div className="web-comment">
      <WebAvatar person={{ initials: item.initials, name: item.author }} size={32.375} />
      <div className="web-comment-main">
        <div className="web-comment-head">
          <span className="web-comment-author">{item.author}</span>
          <span className="web-comment-time">{item.time}</span>
        </div>
        <div className="web-comment-body">{item.body}</div>
        {/* EXP-741: the card is the thread — the reply row closes it. */}
        <div className="web-comment-replies">
          <button className="web-reply-row" type="button">
            Leave a reply…
          </button>
        </div>
      </div>
    </div>
  )
}

/* The properties card (issue-properties-panel.tsx): ghost `size=xs` pills in
   a rounded-xl glass card at the top of the reading column. */
function PropsCard({ issue }: { issue: Issue }) {
  return (
    <div className="web-propsband">
      <div className="web-propscard">
        <button className="web-prop is-click" type="button">
          <StatusGlyph status={issue.status} size={ICON_3} />
          {STATUS_LABEL[issue.status]}
        </button>
        <button className="web-prop is-click" type="button">
          <PriorityGlyph priority={issue.priority} size={ICON_3} />
          {PRIORITY_LABEL[issue.priority]}
        </button>
        <button className="web-prop is-click" type="button">
          <WebAvatar person={issue.assignee} size={ICON_4} />
          {issue.assignee ? issue.assignee.name : `Unassigned`}
        </button>
        <button className="web-prop is-click" type="button">
          <IcTag size={ICON_3} />
          {issue.labels?.length ? (
            issue.labels.map((l) => (
              <span key={l.name} className="web-prop-labels">
                <span className="web-label-dot" style={{ background: l.color }} />
                {l.name}
              </span>
            ))
          ) : (
            <span>Label</span>
          )}
        </button>
        <button className="web-prop is-click" type="button">
          <IcCalendar size={ICON_3} />
          {issue.due ?? `Due date`}
        </button>
        <span className="web-boardchip">
          <IcCode size={ICON_35} style={{ color: WEB_BOARD.color }} />
          {WEB_BOARD.name}
        </span>
      </div>
    </div>
  )
}

export function WebIssueDetail({ issueId }: { issueId: string }) {
  const { interactive, closeIssue } = useWeb()
  const issue = getIssue(issueId)
  const session = AGENT_SESSIONS.find((s) => s.issueId === issue.id)
  const [draft, setDraft] = useState(``)
  const [extra, setExtra] = useState<ActivityItem[]>([])

  const submit = () => {
    const body = draft.trim()
    if (!body) return
    setExtra((prev) => [
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

  const activity = ISSUE_ACTIVITY[issue.id] ?? []
  const index = Math.max(0, ISSUES.findIndex((i) => i.id === issue.id)) + 1

  return (
    <div className="web-page">
      <div className="web-crumbs">
        <button
          className={`web-crumb-board${interactive ? ` is-click` : ``}`}
          type="button"
          onClick={interactive ? closeIssue : undefined}
        >
          <IcCode size={ICON_35} style={{ color: WEB_BOARD.color }} />
          <span className="web-crumb-boardname">{WEB_BOARD.name}</span>
        </button>
        <IcChevRight size={ICON_3} className="web-crumb-sep" />
        <span className="web-crumb-id">{issue.id}</span>
        <IcChevRight size={ICON_3} className="web-crumb-sep" />
        <span className="web-crumb-title">{issue.title}</span>
        <div className="web-crumb-actions">
          <span className="web-position">
            {index} / {ISSUES.length}
          </span>
          <button className="web-icbtn is-click" type="button" title="Previous issue">
            <IcChevUp size={ICON_4} />
          </button>
          <button className="web-icbtn is-click" type="button" title="Next issue">
            <IcChevDown size={ICON_4} />
          </button>
          <span className="web-vrule" />
          <button className="web-icbtn is-glass is-click" type="button" title="Copy link to issue">
            <IcLink2 size={ICON_4} />
          </button>
          <button className="web-icbtn is-glass is-click" type="button" title="Delete issue">
            <IcTrash size={ICON_4} />
          </button>
        </div>
      </div>

      <div className="web-detail-scroll">
        <div className="web-col">
          <div className="web-issue-title">{issue.title}</div>
          <PropsCard issue={issue} />

          {session && (
            <div className="web-codingstack">
              <div className="web-glassrow">
                <span className="web-codingbadge">
                  <span className="web-codingdot" />
                  Coding now
                </span>
                <span className="web-codingwho">
                  {`${WEB_USER.name} · ${session.device}`}
                </span>
                {/* Pill mode="action" primary — the accent fill for the one
                    call to action in the row; the glyph is `nav-devices`
                    (issue-coding-rows.tsx), never a raw eye. */}
                <button
                  className="web-outlinebtn is-primary is-click"
                  type="button"
                >
                  <IcDevices size={ICON_3} />
                  Watch
                </button>
              </div>
            </div>
          )}

          {/* EXP-736: the Relations card follows the coding strip
              (issue-relations-card.tsx) — header + "Add relation" stand alone
              above an empty list, which is what makes the affordance
              discoverable. The glyph is `relation-section` (link-2). */}
          <div className="web-relations">
            <div className="web-relcard">
              <div className="web-relhead">
                <IcLink2 size={ICON_35} className="web-relhead-icon" />
                <span>Relations</span>
                <button className="web-outlinebtn is-click" type="button">
                  <IcPlus size={ICON_3} />
                  Add relation
                </button>
              </div>
            </div>
          </div>

          <div className="web-editor">
            <Description issueId={issue.id} />
            <div className="web-editrail">
              <span className="web-editrail-btn">
                <IcSmile size={ICON_4} />
              </span>
              <span className="web-editrail-btn">
                <IcImage size={ICON_4} />
              </span>
              <span className="web-editrail-btn">
                <IcPaperclip size={ICON_4} />
              </span>
            </div>
          </div>

          <div className="web-timeline">
            <div className="web-timeline-head">{`Activity (${activity.length + extra.length})`}</div>
            <EventRow actor={WEB_USER.name} text="created the issue" time="3 days ago" />
            {activity.map((item, i) => (
              <ActivityRow key={i} item={item} />
            ))}
            {extra.map((item, i) => (
              <ActivityRow key={`x${i}`} item={item} />
            ))}
            {/* comment-composer.tsx: a rounded-2xl muted card whose footer
                row carries the insert affordances and the send button. */}
            <div className="web-composer">
              <textarea
                className="web-composer-input"
                placeholder="Leave a reply…"
                rows={2}
                value={draft}
                readOnly={!interactive}
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={
                  interactive
                    ? (e) => {
                        if (e.key === `Enter` && (e.metaKey || e.ctrlKey)) {
                          e.preventDefault()
                          submit()
                        }
                      }
                    : undefined
                }
              />
              <div className="web-composer-foot">
                <span className="web-editrail-btn">
                  <IcImage size={ICON_4} />
                </span>
                <span className="web-editrail-btn">
                  <IcPaperclip size={ICON_4} />
                </span>
                <span className="web-editrail-btn">
                  <IcHash size={ICON_4} />
                </span>
                <span className="web-editrail-btn">
                  <IcSmile size={ICON_4} />
                </span>
                <button
                  className={`web-send${interactive && draft.trim() ? ` is-click` : ``}`}
                  type="button"
                  disabled={!draft.trim()}
                  onClick={interactive ? submit : undefined}
                  title="Send comment"
                  aria-label="Send comment"
                >
                  <IcSubmit size={27.75} />
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
      <WebAgentDock />
    </div>
  )
}
