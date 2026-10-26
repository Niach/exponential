/* ─── THE thread screen (thread_screen.rs, EXP-1294/1316/1334, NEW-6/46,
   N-12) for a thread with ONE session — the one a Start opens:
   · the HEADER (44px sides): the derived title "#EXP-8 <title>" (21px
     semibold), the Stop pill while the chat is not ended (chats.json STOP:
     `sessions.end` on the root), then the `…` chat menu; under it THE
     strip — `+` · Conversation · the linked issue chip · … · Guide pinned
     right after a divider — then a hairline;
   · the TRANSCRIPT, the one scroll, in the 896px work column: the agent's
     prose beside its sparkle mark, THE Trace lines between; a WAITING
     question is ALWAYS inline at its place too (chats.json FEED-140: the
     AskCard with its option rows, the `Other…` row and Answer);
   · THE Dock floating over its bottom edge: the waiting Question card
     (dock.json — a tap opens its bottom sheet with the option rows), then
     THE composer card: the field over ONE row — `+` · the device glyph ·
     the model pin · Plan … Send, which turns Stop (`ui-stop`, the TURN
     interrupt) while a turn runs. ─── */
import { useEffect, useRef, useState, type KeyboardEvent } from "react"
import { createPortal } from "react-dom"
import { RUN_MODEL, getIssue, type FeedRow, type TraceIcon } from "./data"
import { useIde } from "./state"
import { StatusIcon } from "./bits"
import {
  IcAgent,
  IcAssistant,
  IcChevDown,
  IcChevRight,
  IcCircleX,
  IcClaude,
  IcCode,
  IcEllipsis,
  IcMonitor,
  IcPlus,
  IcQuestion,
  IcStop,
  IcSubmit,
  IcTraceEdit,
  IcTraceRead,
  IcTraceReasoning,
  IcTraceSubagent,
  IcTraceTerminal,
  IcX,
  type IdeIcon,
} from "./icons"

const TRACE_GLYPH: Record<TraceIcon, IdeIcon> = {
  reasoning: IcTraceReasoning,
  read: IcTraceRead,
  edit: IcTraceEdit,
  terminal: IcTraceTerminal,
  subagent: IcTraceSubagent,
  failed: IcCircleX,
}

/* fixtures/trace.json `sm`: 14px icon, 6px gap, 12/18 muted title; a
   collapsible line trails the chevron 4px after; `error` = destructive. */
function TraceLine({ row }: { row: Extract<FeedRow, { kind: `trace` }> }) {
  const Glyph = TRACE_GLYPH[row.icon]
  return (
    <div className={`ide-trace${row.error ? ` is-error` : ``}`}>
      <Glyph size={14} />
      <span className="ide-trace-title">{row.title}</span>
      {row.description && <span className="ide-trace-desc">{row.description}</span>}
      {row.chevron && <IcChevRight size={14} className="ide-trace-chev" />}
    </div>
  )
}

function Row({ row }: { row: FeedRow }) {
  if (row.kind === `trace`) return <TraceLine row={row} />
  if (row.kind === `user`) {
    return (
      <div className="ide-bubble-wrap">
        <div className="ide-bubble">{row.text}</div>
        <div className="ide-bubble-cap">Danny Strähhuber · 16:37</div>
      </div>
    )
  }
  return (
    <div className="ide-prose">
      <IcAssistant size={10.5} className="ide-prose-mark" />
      <p>{row.text}</p>
    </div>
  )
}

/* steer_viewer.rs `render_question` in the transcript: THE AskCard (the
   opaque card fill, the amber help glyph beside the question), the option
   rows (label over its muted description), the free-text `Other…` row and
   the Answer button (plan-card.json `custom` / `answer`), dimmed until
   something is typed. A pick answers at once. */
function InlineQuestion() {
  const { thread, answer, interactive } = useIde()
  const [other, setOther] = useState(``)
  const question = thread?.question
  if (!question || thread?.state !== `needs_you`) return null
  const submitOther = () => {
    if (other.trim()) answer(other.trim())
  }
  return (
    <div
      style={{
        border: `1px solid var(--g-stroke-card)`,
        background: `var(--g-card-opaque)`,
        borderRadius: 14,
        padding: `10.5px 12.25px`,
        display: `flex`,
        flexDirection: `column`,
        gap: 7,
      }}
    >
      <div style={{ display: `flex`, alignItems: `flex-start`, gap: 7 }}>
        <IcQuestion size={14} className="ide-c-amber" style={{ marginTop: 3, flex: `none` }} />
        <p style={{ margin: 0, fontSize: `var(--f-base)`, lineHeight: `22px` }}>{question.text}</p>
      </div>
      <div className="ide-ask-options" style={{ marginTop: 0, paddingLeft: 21 }}>
        {question.options.map((option) => (
          <button
            key={option.label}
            className={`ide-ask-option${interactive ? ` is-click` : ``}`}
            type="button"
            onClick={interactive ? () => answer(option.label) : undefined}
          >
            <span className="ide-ask-option-text">
              <span className="ide-ask-option-label">{option.label}</span>
              <span className="ide-ask-option-desc">{option.description}</span>
            </span>
          </button>
        ))}
        <input
          className="ide-ask-option"
          style={{ background: `transparent`, color: `inherit`, fontSize: `var(--f-xs)`, outline: `none` }}
          placeholder="Other…"
          value={other}
          readOnly={!interactive}
          onChange={(e) => setOther(e.target.value)}
          onKeyDown={interactive ? (e) => e.key === `Enter` && submitOther() : undefined}
        />
        <div style={{ display: `flex`, justifyContent: `flex-end` }}>
          <button
            className={`ide-pill is-sm is-action${interactive && other.trim() ? ` is-click` : ``}`}
            type="button"
            disabled={!other.trim()}
            style={other.trim() ? undefined : { opacity: 0.5 }}
            onClick={interactive ? submitOther : undefined}
          >
            Answer
          </button>
        </div>
      </div>
    </div>
  )
}

/* dock.rs `dock_card` + the sheet it opens (ask_card's option rows). */
function QuestionDock() {
  const { thread, answer, interactive } = useIde()
  const [open, setOpen] = useState(false)
  const cardRef = useRef<HTMLButtonElement>(null)
  const question = thread?.question
  if (!question || thread?.state !== `needs_you`) return null
  /* The sheet's scrim covers the WINDOW: it portals to the demo root. */
  const root = open ? cardRef.current?.closest(`.ide-root`) : null
  return (
    <>
      <button
        ref={cardRef}
        className={`ide-dockcard${interactive ? ` is-click` : ``}`}
        type="button"
        onClick={interactive ? () => setOpen(true) : undefined}
      >
        <IcQuestion size={14} className="ide-c-amber ide-dockcard-icon" />
        <span className="ide-dockcard-main">
          <span className="ide-dockcard-label ide-c-amber">Question</span>
          <span className="ide-dockcard-title">
            <span className="ide-flex1 ide-truncate">{question.text}</span>
            <IcChevRight size={14} className="ide-c-muted" />
          </span>
        </span>
      </button>
      {root &&
        createPortal(
        <div className="ide-sheet-scrim" onClick={() => setOpen(false)}>
          <div className="ide-sheet" onClick={(e) => e.stopPropagation()}>
            <div className="ide-sheet-head">
              <IcQuestion size={14} className="ide-c-amber" />
              <span className="ide-dockcard-label ide-c-amber">Question</span>
              <span className="ide-flex1" />
              <button className="ide-strip-new is-click" type="button" aria-label="Close" onClick={() => setOpen(false)}>
                <IcX size={10.5} />
              </button>
            </div>
            <div className="ide-sheet-body">
              <p className="ide-sheet-text">{question.text}</p>
              <div className="ide-ask-options">
                {question.options.map((option, i) => (
                  <button
                    key={option.label}
                    className="ide-ask-option is-click"
                    type="button"
                    onClick={() => {
                      setOpen(false)
                      answer(option.label)
                    }}
                  >
                    <span className="ide-ask-option-text">
                      <span className="ide-ask-option-label">{option.label}</span>
                      <span className="ide-ask-option-desc">{option.description}</span>
                    </span>
                    <span className="ide-keychip">{i + 1}</span>
                  </button>
                ))}
              </div>
            </div>
          </div>
        </div>,
          root,
        )}
    </>
  )
}

function SessionComposer() {
  const { thread, steer, stopTurn, interactive } = useIde()
  const [draft, setDraft] = useState(``)
  /* The demo types only to an idle (or ended: a message resumes it) session;
     a message sent mid-turn would queue, which the demo does not script. */
  const live = thread !== null && thread.state !== `working`
  /* composer.rs SubmitKind: Send ⇄ Stop — Stop while the turn runs. */
  const stopping = thread?.state === `working`
  const submit = () => {
    if (!draft.trim()) return
    steer(draft)
    setDraft(``)
  }
  const onKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === `Enter` && !e.shiftKey) {
      e.preventDefault()
      submit()
    }
  }
  return (
    <div className="ide-cmp">
      <textarea
        className="ide-cmp-field"
        rows={2}
        placeholder="Type / for commands"
        value={draft}
        readOnly={!interactive || !live}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={interactive ? onKey : undefined}
      />
      <div className="ide-cmp-row">
        <span className="ide-cmp-tool" aria-label="Add">
          <IcPlus size={12.25} />
        </span>
        {/* N-12: the run's controls after the "+": the Device pin as its
            glyph alone, the model pin, the Plan switch. */}
        <span className="ide-cmp-controls">
          <span className="ide-pin">
            <IcMonitor size={12} />
            <IcChevDown size={12} className="ide-pin-chev" />
          </span>
          <span className="ide-pin">
            {RUN_MODEL}
            <IcChevDown size={12} className="ide-pin-chev" />
          </span>
          <span className="ide-pin">
            Plan
            <span className="ide-switch" />
          </span>
        </span>
        <span className="ide-flex1" />
        {stopping ? (
          <button
            className={`ide-send${interactive ? ` is-click` : ``}`}
            type="button"
            aria-label="Stop"
            title="Stop"
            onClick={interactive ? stopTurn : undefined}
          >
            <IcStop size={16} />
          </button>
        ) : (
          <button
            className={`ide-send${interactive ? ` is-click` : ``}`}
            type="button"
            disabled={!draft.trim()}
            aria-label="Send"
            onClick={interactive ? submit : undefined}
          >
            <IcSubmit size={16} />
          </button>
        )}
      </div>
    </div>
  )
}

export function SessionScreen() {
  const { thread, stopThread, interactive } = useIde()
  const scrollRef = useRef<HTMLDivElement>(null)
  const rows = thread?.rows ?? []
  const typing = thread?.typing ?? null
  const working = thread?.state === `working`
  const [secs, setSecs] = useState(0)

  /* The working row's elapsed time ticks in seconds while the turn runs. */
  useEffect(() => {
    if (!working) return undefined
    setSecs(0)
    const t = window.setInterval(() => setSecs((n) => n + 1), 1000)
    return () => window.clearInterval(t)
  }, [working])

  /* Follow the bottom while the transcript streams. */
  useEffect(() => {
    const el = scrollRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [rows.length, typing, thread?.state])

  if (!thread) return null
  const issue = getIssue(thread.issueId)
  const live = thread.state !== `ended`

  return (
    <div className="ide-thread">
      <div className="ide-thread-head">
        <div className="ide-thread-titlerow">
          <h1 className="ide-thread-title">{`#${issue.id} ${issue.title}`}</h1>
          {live && (
            <button
              className={`ide-pill is-md is-action${interactive ? ` is-click` : ``}`}
              type="button"
              title="Stop"
              onClick={interactive ? stopThread : undefined}
            >
              <IcCircleX size={16} className="ide-c-danger" />
              Stop
            </button>
          )}
          {/* thread_screen.rs `guide_menu`: the chat menu's `…` (Stop · Stop
              and delete while live, Delete once ended). */}
          <span className="ide-ghost-btn" title="More">
            <IcEllipsis size={14} />
          </span>
        </div>
        <div className="ide-strip">
          <span className="ide-ghost-btn" title="Link an issue">
            <IcPlus size={14} />
          </span>
          <span className="ide-stripchip is-tab is-active">
            <IcAgent size={10.5} />
            Conversation
          </span>
          <span className="ide-stripchip">
            <StatusIcon status={issue.status} size={10.5} />
            <span className="ide-stripchip-id">{issue.id}</span>
            <span className="ide-stripchip-title">{issue.title}</span>
          </span>
          <span className="ide-flex1" />
          <span className="ide-strip-divider" />
          <span className="ide-stripchip is-tab">
            <IcCode size={10.5} />
            Guide
          </span>
        </div>
        <div className="ide-thread-rule" />
      </div>
      <div className="ide-transcript" ref={scrollRef}>
        <div className="ide-transcript-col">
          {rows.map((row, i) => (
            <Row key={i} row={row} />
          ))}
          {typing !== null && <Row row={{ kind: `narration`, text: typing }} />}
          <InlineQuestion />
          {/* steer_viewer.rs `render_working_row`: the agent's working
              mark and "<verb>… (<elapsed>)" in the tool type, muted. */}
          {working && typing === null && (
            <div className="ide-working">
              <IcClaude size={12} />
              <span>{`Thinking… (${secs}s)`}</span>
            </div>
          )}
        </div>
      </div>
      <div className="ide-dock">
        <div className="ide-dock-col">
          <QuestionDock />
          <SessionComposer />
        </div>
      </div>
    </div>
  )
}
