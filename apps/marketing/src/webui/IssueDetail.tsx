/* ─── The work view: one tab, three faces (EXP-870/877) ───
   Mirrors apps/web issue-detail-view.tsx + sessions/$sessionId.tsx: the ONE
   work header (WorkHeader.tsx — title, Issue | Run | +N -M toggle, pin, "…",
   properties tray) fixed above the face. The issue face is the markdown
   description with the editor's insert rail, "Add sub-issues", and the
   activity timeline (issue-timeline.tsx + comment-rows/*) whose comment
   cards end in the "Leave a reply…" row. The breadcrumb bar and the
   "Coding now" row are gone: the tabs strip and the tray's Stop replaced
   them. */
import { useState } from "react"
import {
  getIssue,
  ISSUE_ACTIVITY,
  ISSUE_BODY,
  type ActivityItem,
  type IssueStatus,
} from "../ide/data"
import { useWeb } from "./state"
import { StatusGlyph, WebAvatar } from "./bits"
import { sessionFor, WEB_USER } from "./data"
import { WebWorkHeader } from "./WorkHeader"
import { WebDiffFace, WebRunFace } from "./RunFace"
import {
  ICON_3,
  ICON_35,
  ICON_4,
  IcHash,
  IcImage,
  IcPaperclip,
  IcPlus,
  IcSmile,
  IcSubmit,
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

function IssueFace({ issueId }: { issueId: string }) {
  const { interactive } = useWeb()
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

  const activity = ISSUE_ACTIVITY[issueId] ?? []

  return (
    <div className="web-detail-scroll">
      <div className="web-workcol">
        <div className="web-editor">
          <Description issueId={issueId} />
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

        {/* sub-issue-composer.tsx: the ghost "Add sub-issues" row. */}
        <button className="web-subissues is-click" type="button">
          <IcPlus size={ICON_35} />
          Add sub-issues
        </button>

        <div className="web-timeline">
          <div className="web-timeline-head">{`Activity (${activity.length + extra.length + 1})`}</div>
          <EventRow actor={WEB_USER.name} text="created the issue" time="3 days ago" />
          {activity.map((item, i) => (
            <ActivityRow key={i} item={item} />
          ))}
          {extra.map((item, i) => (
            <ActivityRow key={`x${i}`} item={item} />
          ))}
          {/* comment-composer.tsx: a rounded-xl card whose footer row
              carries the insert affordances and the send button. */}
          <div className="web-composer">
            <textarea
              className="web-composer-input"
              placeholder="Leave a comment…"
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
  )
}

export function WebIssueDetail({ issueId }: { issueId: string }) {
  const { face: picked } = useWeb()
  const issue = getIssue(issueId)
  /* A face the issue has no run for is hidden, never shown empty. */
  const face = sessionFor(issue.id) ? picked : `issue`
  return (
    <div className="web-page">
      <WebWorkHeader issue={issue} />
      {face === `run` ? (
        <WebRunFace issueId={issue.id} />
      ) : face === `diff` ? (
        <WebDiffFace issueId={issue.id} />
      ) : (
        <IssueFace issueId={issue.id} />
      )}
    </div>
  )
}
