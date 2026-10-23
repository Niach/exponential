/* ─── The issue detail (issue_detail.rs + issue_header.rs + work_header.rs,
   EXP-877/1162/1294): ONE scrolling body in the 896px work column (20px
   gutter), under the compact fixed bar whose right cluster is
   [Start] [pin] [⋯]. The body: the 24/32 semibold title; the Threads row
   while a thread works the issue; the property chips (Status · Priority ·
   Assignee · Labels · Due date · Board — Sm glass pills, unset ones muted);
   the description; the editor's emoji / image / attach glyphs; "Add
   sub-issues"; a hairline; then "Activity (N)" — muted event lines and
   comment CARDS, each closed by its "Leave a reply…" row — and the
   bottom composer. Start opens Home with this issue as the subject. ─── */
import { useState, type KeyboardEvent, type ReactNode } from "react"
import {
  getIssue,
  ISSUE_ACTIVITY,
  ISSUE_BODY,
  PRIORITY_LABEL,
  STATUS_LABEL,
  type ActivityItem,
} from "./data"
import { useIde } from "./state"
import { ACTIVE_BOARD } from "./Rail"
import { Avatar, PriorityIcon, RunMark, StatusIcon } from "./bits"
import {
  IcAssignee,
  IcCalDays,
  IcEllipsis,
  IcImage,
  IcIssueRef,
  IcPaperclip,
  IcPin,
  IcPlay,
  IcPlus,
  IcSmile,
  IcSubmit,
  IcTag,
} from "./icons"

/* pickers.rs `chip_button`: the Sm glass pill; an unset value is muted. */
function Chip({ icon, children, muted }: { icon: ReactNode; children: ReactNode; muted?: boolean }) {
  return (
    <span className="ide-pill is-sm is-action">
      {icon}
      <span className={muted ? `ide-c-muted` : undefined}>{children}</span>
    </span>
  )
}

function Description({ issueId }: { issueId: string }) {
  const body = ISSUE_BODY[issueId]
  if (!body) {
    return <div className="ide-issue-desc is-empty">Add description…</div>
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
              /* EXP-885: a resolved `#EXP-5` is THE issue chip — status
                 glyph · mono identifier · title. */
              <span key={si} className="ide-issuechip">
                <StatusIcon status={getIssue(seg.t).status} size={10.5} />
                <span className="ide-issuechip-id">{seg.t}</span>
                <span className="ide-issuechip-title">{getIssue(seg.t).title}</span>
              </span>
            ) : seg.mention ? (
              <span key={si} className="ide-mention">{`@${seg.t}`}</span>
            ) : (
              <span key={si}>{seg.t}</span>
            ),
          )}
        </p>
      ))}
    </div>
  )
}

/* Events are "<actor> <phrase>" in the fixture; the actor leads in the
   foreground, the phrase and the time stay muted. */
const EVENT_ACTOR = /^(.+?) (changed|added|removed|assigned|opened|merged|moved|created) (.*)$/

function ActivityRow({ item, last }: { item: ActivityItem; last: boolean }) {
  if (item.kind === `event`) {
    const m = EVENT_ACTOR.exec(item.text)
    return (
      <div className={`ide-tl${last ? ` is-last` : ``}`}>
        <div className="ide-tl-gutter">
          <span className="ide-tl-dot" />
        </div>
        <div className="ide-tl-event">
          {m ? (
            <>
              <span className="ide-tl-actor">{m[1]}</span>
              {` ${m[2]} ${m[3]} · ${item.time}`}
            </>
          ) : (
            `${item.text} · ${item.time}`
          )}
        </div>
      </div>
    )
  }
  return (
    <div className={`ide-tl is-comment${last ? ` is-last` : ``}`}>
      <div className="ide-tl-gutter">
        <Avatar person={{ initials: item.initials, name: item.author }} size={24} />
      </div>
      <div className="ide-comment">
        <div className="ide-comment-head">
          <span className="ide-comment-author">{item.author}</span>
          <span className="ide-comment-time">{item.time}</span>
        </div>
        <div className="ide-comment-body">{item.body}</div>
        {/* EXP-741: the card is the thread — the reply row closes it. */}
        <div className="ide-comment-reply">Leave a reply…</div>
      </div>
    </div>
  )
}

export function IssueDetail({ issueId }: { issueId: string }) {
  const { interactive, start, thread, openThread } = useIde()
  const issue = getIssue(issueId)
  const [draft, setDraft] = useState(``)
  const [extraComments, setExtraComments] = useState<ActivityItem[]>([])

  const activity = [...(ISSUE_ACTIVITY[issue.id] ?? []), ...extraComments]
  const issueThread = thread && thread.issueId === issue.id ? thread : null

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
      {/* The compact fixed bar: its collapsed title shows only once the
          title row has scrolled under it, so at rest it is the cluster. */}
      <div className="ide-workbar">
        <div className="ide-workbar-cluster">
          <button
            className={`ide-btn-primary is-lg${interactive ? ` is-click` : ``}`}
            type="button"
            title="Start an agent on this issue"
            onClick={interactive ? () => start(issue.id) : undefined}
          >
            <IcPlay size={16} />
            Start
          </button>
          <span className="ide-ghost-btn" title="Pin">
            <IcPin size={14} />
          </span>
          <span className="ide-ghost-btn" title="More">
            <IcEllipsis size={14} />
          </span>
        </div>
      </div>
      <div className="ide-issue-body">
        <div className="ide-col">
          <h1 className="ide-issue-title">{issue.title}</h1>
          {issueThread && (
            <div className="ide-threads-row">
              <span className="ide-threads-label">Threads</span>
              <button
                className={`ide-pill is-sm is-action${interactive ? ` is-click` : ``}`}
                type="button"
                onClick={interactive ? openThread : undefined}
              >
                <RunMark needsYou={issueThread.state === `needs_you`} size={12} />
                <span className="ide-threads-chip">{`#${issue.id} ${issue.title}`}</span>
              </button>
            </div>
          )}
          <div className="ide-chiprow">
            <Chip icon={<StatusIcon status={issue.status} size={12} />}>{STATUS_LABEL[issue.status]}</Chip>
            <Chip icon={<PriorityIcon priority={issue.priority} size={12} />}>
              {PRIORITY_LABEL[issue.priority]}
            </Chip>
            <Chip icon={<IcAssignee size={12} />} muted={!issue.assignee}>
              {issue.assignee ? issue.assignee.name : `Assignee`}
            </Chip>
            <Chip icon={<IcTag size={12} />} muted={!issue.labels?.length}>
              {issue.labels?.length ? issue.labels.map((l) => l.name).join(`, `) : `Labels`}
            </Chip>
            <Chip icon={<IcCalDays size={12} />} muted={!issue.due}>
              {issue.due ?? `Due date`}
            </Chip>
            <Chip icon={<ACTIVE_BOARD.Icon size={12} style={{ color: ACTIVE_BOARD.color }} />}>
              {ACTIVE_BOARD.name}
            </Chip>
          </div>
          <Description issueId={issue.id} />
          <div className="ide-editor-tools">
            <span className="ide-icon-btn" title="Emoji">
              <IcSmile size={14} />
            </span>
            <span className="ide-icon-btn" title="Image">
              <IcImage size={14} />
            </span>
            <span className="ide-icon-btn" title="Attach file">
              <IcPaperclip size={14} />
            </span>
          </div>
          <span className="ide-addsub">
            <IcPlus size={10.5} />
            Add sub-issues
          </span>
          <div className="ide-rule" />
          <div className="ide-activity-head">{`Activity (${activity.length})`}</div>
          <div className="ide-timeline">
            {activity.map((item, i) => (
              <ActivityRow key={i} item={item} last={i === activity.length - 1} />
            ))}
          </div>
          <div className="ide-commentbox">
            <input
              className="ide-commentbox-input"
              placeholder="Leave a reply…"
              value={draft}
              readOnly={!interactive}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={interactive ? onComposerKey : undefined}
            />
            {/* comments.rs composer_row: Insert image · Attach · Link an
                issue · Emoji, then the round Send. */}
            <div className="ide-commentbox-row">
              <span className="ide-icon-btn" title="Insert image">
                <IcImage size={12.25} />
              </span>
              <span className="ide-icon-btn" title="Attach file">
                <IcPaperclip size={12.25} />
              </span>
              <span className="ide-icon-btn" title="Link an issue">
                <IcIssueRef size={12.25} />
              </span>
              <span className="ide-icon-btn" title="Emoji">
                <IcSmile size={12.25} />
              </span>
              <span className="ide-flex1" />
              <button
                className={`ide-send${interactive ? ` is-click` : ``}`}
                type="button"
                disabled={!draft.trim()}
                aria-label="Send"
                onClick={interactive ? submitComment : undefined}
              >
                <IcSubmit size={16} />
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
