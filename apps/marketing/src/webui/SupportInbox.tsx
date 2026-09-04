/* ─── Support — the 3-pane helpdesk inbox ───
   Mirrors apps/web helpdesk/support-inbox.tsx: a w-80 thread list headed by
   the Open/Resolved capsule segment, the conversation (reporter + thread
   title, Close ticket, glass/primary bubbles, reply-or-internal-note
   composer), and the w-72 details rail — Reporter, widget Context, then the
   Linked issue OR the Escalate board picker, with the lock footnote. */
import { useState, type KeyboardEvent } from "react"
import { getIssue, type Issue } from "../ide/data"
import { useWeb } from "./state"
import { WebAgentDock } from "./Board"
import {
  ICON_3,
  ICON_4,
  IcCheck,
  IcChevDown,
  IcExternalLink,
  IcLifeBuoy,
  IcLock,
  IcMail,
  IcReopen,
  IcSend,
  IcStickyNote,
  IcSupportOpen,
  IcSupportResolved,
} from "./icons"
import {
  getThread,
  SUPPORT_THREADS,
  WEB_USER,
  type SupportMessage,
  type SupportThread,
} from "./data"

/* Exported for the home page's collaboration scene (CollabSection), which
   composes the same real-UI pieces outside the full 3-pane inbox. */
export function Bubble({
  message,
  reporter,
}: {
  message: SupportMessage
  reporter: string
}) {
  const isInbound = message.direction === `inbound`
  const kind = isInbound
    ? ` is-inbound`
    : message.internal
      ? ` is-internal`
      : ` is-reply`
  return (
    <div className={`web-bubble${kind}`}>
      {message.internal && (
        <span className="web-note-badge">
          <IcStickyNote size={11.5} />
          Internal
        </span>
      )}
      <p className="web-bubble-body">{message.body}</p>
      <p className="web-bubble-meta">
        {`${isInbound ? reporter : message.author} · ${message.time}`}
      </p>
    </div>
  )
}

/* The conversation header — reporter name over the THREAD title, with the
   Close-ticket button. Shared with HelpdeskChatDemo. */
export function SupportChatHead({ thread }: { thread: SupportThread }) {
  return (
    <div className="web-sup-chathead">
      <div className="web-sup-chatwho">
        <div className="web-sup-chatname">{thread.reporterName}</div>
        <div className="web-sup-issuetitle">{thread.title}</div>
      </div>
      <button className="web-outlinebtn" type="button">
        {thread.resolved ? <IcReopen size={ICON_3} /> : <IcCheck size={ICON_3} />}
        {thread.resolved ? `Reopen ticket` : `Close ticket`}
      </button>
    </div>
  )
}

/* Presentational details rail (Reporter · Context · Linked issue /
   Escalate) — context-free so the home page's HelpdeskChatDemo can compose
   it outside the full 3-pane inbox. */
export function SupportRail({
  thread,
  issue,
  interactive,
  onOpenIssue,
}: {
  thread: SupportThread
  issue: Issue | null
  interactive: boolean
  onOpenIssue?: () => void
}) {
  return (
    <div className="web-sup-rail">
      {/* Divs, not <section>/<h2>/<p> — the site stylesheet pads bare
          sections (80px), which would blow the rail apart. */}
      <div>
        <div className="web-rail-label">Reporter</div>
        <div className="web-rail-name">{thread.reporterName}</div>
        <div className="web-rail-sub">{thread.reporterEmail}</div>
        <div className="web-rail-sub">{`Last seen ${thread.lastSeen}`}</div>
      </div>
      {thread.context && (
        <div>
          <div className="web-rail-label">Context</div>
          <div className="web-rail-sub">{thread.context.pageUrl}</div>
          <div className="web-rail-sub is-wrap">{thread.context.userAgent}</div>
          <div className="web-rail-sub">{`Viewport ${thread.context.viewport}`}</div>
        </div>
      )}
      {issue ? (
        <div>
          <div className="web-rail-label">Linked issue</div>
          <button
            className={`web-rail-issue${interactive ? ` is-click` : ``}`}
            type="button"
            onClick={interactive ? onOpenIssue : undefined}
          >
            {issue.id}
            <IcExternalLink size={ICON_3} />
          </button>
          <div className="web-rail-sub">{issue.title}</div>
        </div>
      ) : (
        <div>
          <div className="web-rail-label">Escalate</div>
          <div className="web-rail-sub is-wrap">
            Create an issue from this ticket on one of the team&rsquo;s boards.
          </div>
          <div className="web-rail-escalate">
            <button className="web-rail-select" type="button">
              Pick a board
              <IcChevDown size={ICON_4} />
            </button>
            <button className="web-rail-create" type="button" disabled>
              Create issue
            </button>
          </div>
        </div>
      )}
      <div className="web-rail-foot">
        <div className="web-rail-lock">
          <IcLock size={ICON_3} />
          Replies are emailed to the reporter with a private conversation link.
        </div>
      </div>
    </div>
  )
}

/* Presentational thread-list row — shared with CollabSection. */
export function SupportThreadRow({
  thread,
  unread,
  selected,
  interactive,
  onClick,
}: {
  thread: SupportThread
  unread: boolean
  selected: boolean
  interactive: boolean
  onClick?: () => void
}) {
  return (
    <button
      type="button"
      className={`web-sup-row${selected ? ` is-selected` : ``}${interactive ? ` is-click` : ``}`}
      onClick={interactive ? onClick : undefined}
    >
      {/* EXP-715: the ticket SUBJECT leads on every client; the reporter and
          the latest public message sit under it. The stamp keeps its own
          6rem slot and the unread dot its 8px one, so rows line up. */}
      <span className="web-sup-row1">
        <span className={`web-sup-subject${unread ? ` is-unread` : ``}`}>
          {thread.title}
        </span>
        <span className="web-sup-time">{thread.lastSeen}</span>
        <span className="web-sup-dotslot">
          {unread && <span className="web-sup-dot" />}
        </span>
      </span>
      <span className="web-sup-preview">
        {`${thread.reporterName} · ${thread.messages[thread.messages.length - 1]?.body ?? ``}`}
      </span>
    </button>
  )
}

/* Reply / internal-note composer — shared with HelpdeskChatDemo. */
export function SupportComposer({
  reporterName,
  mode,
  setMode,
  draft,
  setDraft,
  onSend,
  interactive,
}: {
  reporterName: string
  mode: `reply` | `note`
  setMode?: (mode: `reply` | `note`) => void
  draft: string
  setDraft?: (draft: string) => void
  onSend?: () => void
  interactive: boolean
}) {
  const onComposerKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === `Enter` && !e.shiftKey) {
      e.preventDefault()
      onSend?.()
    }
  }
  return (
    <div className="web-sup-composer">
      {/* EXP-698: the ONE composer card (composer.tsx) — the Reply/Note toggle
          is its LEADING row, the field sits inside it, and the round submit
          glyph closes the tool row. Note mode tints only the hairline. */}
      <div className={`web-sup-card${mode === `note` ? ` is-note` : ``}`}>
        <div className="web-sup-modes">
          <button
            type="button"
            className={`web-modepill${mode === `reply` ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
            onClick={interactive ? () => setMode?.(`reply`) : undefined}
          >
            <IcMail size={ICON_3} />
            Reply
          </button>
          <button
            type="button"
            className={`web-modepill is-note${mode === `note` ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
            onClick={interactive ? () => setMode?.(`note`) : undefined}
          >
            <IcStickyNote size={ICON_3} />
            Internal note
          </button>
        </div>
        <textarea
          className="web-sup-input"
          rows={2}
          placeholder={
            mode === `reply`
              ? `Reply to ${reporterName}… (emailed to them)`
              : `Add an internal note… (never sent to the reporter)`
          }
          value={draft}
          readOnly={!interactive}
          onChange={(e) => setDraft?.(e.target.value)}
          onKeyDown={interactive ? onComposerKey : undefined}
        />
        <div className="web-sup-inputrow">
          <button
            className={`web-sup-send${interactive && draft.trim() ? ` is-click` : ``}`}
            type="button"
            disabled={!draft.trim()}
            onClick={interactive ? onSend : undefined}
            title={mode === `reply` ? `Send reply` : `Save note`}
            aria-label={mode === `reply` ? `Send reply` : `Save note`}
          >
            <IcSend size={27.75} />
          </button>
        </div>
      </div>
    </div>
  )
}

export function WebSupportInbox() {
  const {
    interactive,
    selectedThreadId,
    selectThread,
    threadFilter,
    setThreadFilter,
    threadRead,
    setNav,
    openIssue,
  } = useWeb()
  const [mode, setMode] = useState<`reply` | `note`>(`reply`)
  const [draft, setDraft] = useState(``)
  const [extraMessages, setExtraMessages] = useState<
    Record<string, SupportMessage[]>
  >({})

  const visible = SUPPORT_THREADS.filter((t) =>
    threadFilter === `resolved` ? t.resolved : !t.resolved
  )
  const thread = selectedThreadId ? getThread(selectedThreadId) : null
  const inFilter = thread ? visible.some((t) => t.id === thread.id) : false
  const shown = inFilter ? thread : null
  const issue = shown?.issueId ? getIssue(shown.issueId) : null
  const messages = shown
    ? [...shown.messages, ...(extraMessages[shown.id] ?? [])]
    : []

  const send = () => {
    const body = draft.trim()
    if (!body || !shown) return
    const message: SupportMessage = {
      direction: `outbound`,
      internal: mode === `note` || undefined,
      author: WEB_USER.name,
      body,
      time: `just now`,
    }
    setExtraMessages((prev) => ({
      ...prev,
      [shown.id]: [...(prev[shown.id] ?? []), message],
    }))
    setDraft(``)
  }

  return (
    <div className="web-page">
      <div className="web-sup is-inbox">
      {/* Left — thread list */}
      <div className="web-sup-list">
        <div className="web-sup-listhead">
          <div className="web-seg is-sm">
            {([`open`, `resolved`] as const).map((tab) => {
              const Icon = tab === `open` ? IcSupportOpen : IcSupportResolved
              return (
                <button
                  key={tab}
                  type="button"
                  className={`web-seg-btn${threadFilter === tab ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
                  onClick={interactive ? () => setThreadFilter(tab) : undefined}
                >
                  <Icon size={ICON_3} />
                  {tab === `open` ? `Open` : `Resolved`}
                </button>
              )
            })}
          </div>
        </div>
        <div className="web-sup-threads">
          {visible.map((t) => (
            <SupportThreadRow
              key={t.id}
              thread={t}
              unread={Boolean(t.unread) && !threadRead.has(t.id)}
              selected={t.id === selectedThreadId}
              interactive={interactive}
              onClick={() => selectThread(t.id)}
            />
          ))}
        </div>
      </div>

      {/* Middle — conversation */}
      {shown ? (
        <div className="web-sup-chat">
          <SupportChatHead thread={shown} />
          <div className="web-sup-msgs">
            {messages.map((m, i) => (
              <Bubble key={i} message={m} reporter={shown.reporterName} />
            ))}
          </div>
          <SupportComposer
            reporterName={shown.reporterName}
            mode={mode}
            setMode={setMode}
            draft={draft}
            setDraft={setDraft}
            onSend={send}
            interactive={interactive}
          />
        </div>
      ) : (
        <div className="web-sup-empty">
          <IcLifeBuoy size={37} />
          <span>Select a conversation</span>
        </div>
      )}

      {/* Right — details rail (Reporter · Context · Linked issue / Escalate) */}
      {shown && (
        <SupportRail
          thread={shown}
          issue={issue}
          interactive={interactive}
          onOpenIssue={
            issue
              ? () => {
                  setNav(`project`)
                  openIssue(issue.id)
                }
              : undefined
          }
        />
      )}
      </div>
      <WebAgentDock />
    </div>
  )
}
