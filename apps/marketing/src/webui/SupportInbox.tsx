/* ─── Support — Featurebase-style 3-pane helpdesk inbox ───
   Mirrors apps/web helpdesk/support-inbox.tsx: thread list (Open/Resolved
   pills, unread indigo dot), conversation (inbound bubbles on --bg-soft,
   outbound replies on --accent, amber-tinted internal notes), reply /
   internal-note composer, details rail with reporter + linked issue. */
import { useState, type KeyboardEvent } from "react"
import { getIssue, PRIORITY_LABEL, STATUS_LABEL } from "../ide/data"
import { useWeb } from "./state"
import { PriorityIcon, StatusIcon } from "../ide/bits"
import { IcCheck, IcSend } from "../ide/icons"
import {
  IcExternalLink,
  IcLifeBuoy,
  IcLock,
  IcMail,
  IcStickyNote,
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
          <IcStickyNote size={10} />
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
      <span className="web-sup-row1">
        <span className="web-sup-name">{thread.reporterName}</span>
        <span className="web-sup-time">{thread.time}</span>
        {unread && <span className="web-sup-dot" />}
      </span>
      <span className="web-sup-preview">
        {thread.messages[thread.messages.length - 1]?.body}
      </span>
    </button>
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
  const issue = shown ? getIssue(shown.issueId) : null
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

  const onComposerKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === `Enter` && !e.shiftKey) {
      e.preventDefault()
      send()
    }
  }

  return (
    <div className="web-sup">
      {/* Left — thread list */}
      <div className="web-sup-list">
        <div className="web-sup-listhead">
          <span className="web-sup-h1">Support</span>
          {([`open`, `resolved`] as const).map((tab) => (
            <button
              key={tab}
              type="button"
              className={`web-tab is-small${threadFilter === tab ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
              onClick={interactive ? () => setThreadFilter(tab) : undefined}
            >
              {tab === `open` ? `Open` : `Resolved`}
            </button>
          ))}
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
      {shown && issue ? (
        <div className="web-sup-chat">
          <div className="web-sup-chathead">
            <div className="web-sup-chatwho">
              <span className="web-sup-name">{shown.reporterName}</span>
              <span className="web-sup-issuetitle">{issue.title}</span>
            </div>
            <button className="web-btn-outline" type="button">
              <IcCheck size={12} />
              {shown.resolved ? `Reopen ticket` : `Close ticket`}
            </button>
          </div>
          <div className="web-sup-msgs">
            {messages.map((m, i) => (
              <Bubble key={i} message={m} reporter={shown.reporterName} />
            ))}
          </div>
          <div className="web-sup-composer">
            <div className="web-sup-modes">
              <button
                type="button"
                className={`web-modepill${mode === `reply` ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
                onClick={interactive ? () => setMode(`reply`) : undefined}
              >
                <IcMail size={12} />
                Reply
              </button>
              <button
                type="button"
                className={`web-modepill is-note${mode === `note` ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
                onClick={interactive ? () => setMode(`note`) : undefined}
              >
                <IcStickyNote size={12} />
                Internal note
              </button>
            </div>
            <div className="web-sup-inputrow">
              <textarea
                className={`web-composer-input${mode === `note` ? ` is-note` : ``}`}
                rows={2}
                placeholder={
                  mode === `reply`
                    ? `Reply to ${shown.reporterName}… (emailed to them)`
                    : `Add an internal note… (never sent to the reporter)`
                }
                value={draft}
                readOnly={!interactive}
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={interactive ? onComposerKey : undefined}
              />
              <button
                className={`web-send${interactive && draft.trim() ? ` is-click` : ``}`}
                type="button"
                disabled={!draft.trim()}
                onClick={interactive ? send : undefined}
                title={mode === `reply` ? `Send reply` : `Save note`}
              >
                <IcSend size={14} />
              </button>
            </div>
          </div>
        </div>
      ) : (
        <div className="web-sup-empty">
          <IcLifeBuoy size={26} className="ide-c-muted" />
          <span>Select a conversation</span>
        </div>
      )}

      {/* Right — details rail */}
      {shown && issue && (
        <div className="web-sup-rail">
          {/* Divs, not <section>/<h2>/<p> — the site stylesheet pads bare
              sections (80px), which would blow the rail apart. */}
          <div>
            <div className="web-rail-label">Reporter</div>
            <div className="web-rail-name">{shown.reporterName}</div>
            <div className="web-rail-sub">{shown.reporterEmail}</div>
            <div className="web-rail-sub">{`Last seen ${shown.lastSeen}`}</div>
          </div>
          <div>
            <div className="web-rail-label">Linked issue</div>
            <div className="web-rail-issue">
              {issue.id}
              <IcExternalLink size={11} className="ide-c-muted" />
            </div>
            <div className="web-rail-sub">{issue.title}</div>
            <div className="web-rail-props">
              <button className="web-prop-btn" type="button">
                <StatusIcon status={issue.status} size={13} />
                {STATUS_LABEL[issue.status]}
              </button>
              <button className="web-prop-btn" type="button">
                <PriorityIcon priority={issue.priority} size={13} />
                {PRIORITY_LABEL[issue.priority]}
              </button>
            </div>
            <button
              className={`web-btn-outline web-rail-view${interactive ? ` is-click` : ``}`}
              type="button"
              onClick={
                interactive
                  ? () => {
                      setNav(`project`)
                      openIssue(issue.id)
                    }
                  : undefined
              }
            >
              View issue
            </button>
          </div>
          <div className="web-rail-foot">
            <div className="web-rail-lock">
              <IcLock size={11} />
              Replies are emailed to the reporter with a private conversation
              link.
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
