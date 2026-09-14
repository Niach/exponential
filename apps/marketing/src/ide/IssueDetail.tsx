/* ─── The Issue face of a top tab (issue_detail.rs, EXP-760/877): the ONE
   work header (title, face toggle, pin, menu, the glass property tray with
   Merge PR and the coding action) fixed over a scrolling body — the
   description, "Add sub-issues", the attachments rail and the Activity
   timeline (muted event lines, comment CARDS each ending in "Leave a
   reply…", the composer). Everything caps to the 896px work column. ─── */
import { useState, type KeyboardEvent } from "react"
import {
  getIssue,
  ISSUE_ACTIVITY,
  ISSUE_BODY,
  type ActivityItem,
} from "./data"
import { useIde } from "./state"
import { Avatar, StatusIcon } from "./bits"
import { WorkHeader } from "./WorkHeader"
import { IcEllipsis, IcImage, IcPaperclip, IcPlus, IcSmile } from "./icons"

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

export function IssueDetail({ issueId }: { issueId: string }) {
  const { interactive } = useIde()
  const issue = getIssue(issueId)
  const [draft, setDraft] = useState(``)
  const [extraComments, setExtraComments] = useState<ActivityItem[]>([])

  const baseActivity = ISSUE_ACTIVITY[issue.id] ?? []
  const activity = [...baseActivity, ...extraComments]

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
      <WorkHeader tabKey={`issue:${issue.id}`} />
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
