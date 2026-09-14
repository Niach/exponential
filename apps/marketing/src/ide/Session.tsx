/* ─── The Run face and the Diff face of a top tab (session_screen.rs +
   diff_pane.rs, EXP-746/787/870/877). Both sit under the SAME work header as
   the Issue face — there is no session header, no Back button and no side
   pane any more.

   Run: the ACP transcript in the work column (narration, tool rows,
   collapsed tool groups, the question card you answer IN the card), then the
   one-row steer composer with its inline send and, under it, the footer
   (attach · model pin · context ring). The composer hides while a question
   is pending (EXP-820) and once the run has ended.

   Diff: the run's changes as a FULL PAGE (EXP-877), file cards in the same
   work column; the fixture file expands into the side-by-side hunk. ─── */
import { useEffect, useRef, useState, type KeyboardEvent } from "react"
import { CONTEXT_PERCENT, RUN_DEVICE, type FeedRow } from "./data"
import { useIde, type RunView } from "./state"
import { WorkHeader } from "./WorkHeader"
import { DiffView } from "./Diff"
import {
  IcChevDown,
  IcChevRight,
  IcCircleArrowUp,
  IcCircleQuestion,
  IcPlus,
  IcSparkles,
  IcWrench,
} from "./icons"

/* steer::feed rows. Prose takes the body rung (14/22), everything else the
   tool rung (12/18); the glyph rides the reading column's left edge. */
function FeedItem({
  row,
  partial,
  onAnswer,
}: {
  row: FeedRow
  partial?: number
  onAnswer?: (answer: string) => void
}) {
  if (row.kind === `narration`) {
    const text = partial === undefined ? row.text : row.text.slice(0, partial)
    return (
      <div className="ide-feed-row is-prose">
        <IcSparkles size={12} className="ide-feed-glyph" />
        <div className="ide-feed-text">
          {text}
          {partial !== undefined && <span className="ide-caret" />}
        </div>
      </div>
    )
  }
  if (row.kind === `user`) {
    return (
      <div className="ide-feed-row is-user">
        <div className="ide-feed-user">{row.text}</div>
      </div>
    )
  }
  if (row.kind === `tool`) {
    return (
      <div className="ide-feed-row is-tool">
        <IcWrench size={11} className="ide-feed-glyph" />
        <span className="ide-feed-verb">{row.verb}</span>
        <span className="ide-feed-target">{row.target}</span>
        {row.detail && (
          <>
            <span className="ide-feed-sep">·</span>
            <span className="ide-feed-detail">{row.detail}</span>
          </>
        )}
      </div>
    )
  }
  if (row.kind === `group`) {
    /* A collapsed run of tool calls — the chevron opens it. */
    return (
      <div className="ide-feed-row is-tool">
        <IcChevRight size={11} className="ide-feed-chev" />
        <IcWrench size={11} className="ide-feed-glyph" />
        <span className="ide-feed-caption">{row.caption}</span>
      </div>
    )
  }
  return (
    <div className={`ide-answercard${onAnswer ? `` : ` is-answered`}`}>
      <IcCircleQuestion size={12} className="ide-answercard-glyph" />
      <div className="ide-answercard-text">{row.text}</div>
      {row.options.map((option, i) => (
        /* EXP-788/820: numbered option buttons, answered in the card. */
        <button
          className={`ide-answeropt${onAnswer ? ` is-click` : ``}`}
          type="button"
          key={option.title}
          disabled={!onAnswer}
          onClick={onAnswer ? () => onAnswer(option.title) : undefined}
        >
          <span className="ide-answeropt-key">{i + 1}</span>
          <span className="ide-answeropt-body">
            <span className="ide-answeropt-title">{option.title}</span>
            <span className="ide-answeropt-sub">{option.sub}</span>
          </span>
        </button>
      ))}
    </div>
  )
}

/* usage_sheet::context_ring — the run's context window, a small meter. */
function ContextRing({ percent }: { percent: number }) {
  const r = 6
  const c = 2 * Math.PI * r
  return (
    <span className="ide-ctxring" title={`Context ${percent}% used`}>
      <svg width={16} height={16} viewBox="0 0 16 16" aria-hidden>
        <circle cx="8" cy="8" r={r} fill="none" stroke="rgba(255,255,255,0.14)" strokeWidth="2" />
        <circle
          cx="8"
          cy="8"
          r={r}
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeDasharray={`${(c * percent) / 100} ${c}`}
          strokeLinecap="round"
          transform="rotate(-90 8 8)"
        />
      </svg>
      <span>{`${percent}%`}</span>
    </span>
  )
}

function SteerComposer({ run }: { run: RunView }) {
  const { interactive, steer } = useIde()
  const [draft, setDraft] = useState(``)
  const send = () => {
    if (!draft.trim()) return
    steer(run.id, draft)
    setDraft(``)
  }
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === `Enter`) send()
  }
  return (
    <div className="ide-steerwrap">
      <div className="ide-workcol">
        {/* EXP-877: ONE row — the field and the inline round send. */}
        <div className="ide-steer">
          <input
            className="ide-steer-input"
            placeholder="Message the agent… (/ for commands)"
            value={draft}
            readOnly={!interactive}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={interactive ? onKey : undefined}
          />
          <button
            className={`ide-steer-send${interactive ? ` is-click` : ``}`}
            type="button"
            title="Send"
            onClick={interactive ? send : undefined}
          >
            <IcCircleArrowUp size={20} />
          </button>
        </div>
        {/* steer_viewer::render_composer_footer — attach · model · ring. */}
        <div className="ide-steer-footer">
          <span className="ide-steer-tool" title="Attach image">
            <IcPlus size={13} />
          </span>
          <div className="ide-flex1" />
          <span className="ide-steer-model">
            {run.model}
            {run.agent === `claude` && <IcChevDown size={10} />}
          </span>
          <ContextRing percent={run.state === `running` ? Math.round(CONTEXT_PERCENT * 0.6) : CONTEXT_PERCENT} />
        </div>
      </div>
    </div>
  )
}

function useTabRun(tabKey?: string): RunView | null {
  const { runs, active, runForTab } = useIde()
  const key = tabKey ?? active
  return (key ? runForTab(key) : null) ?? runs[0] ?? null
}

export function SessionScreen({ tabKey }: { tabKey?: string } = {}) {
  const { answerQuestion, interactive } = useIde()
  const run = useTabRun(tabKey)
  const feedRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    const el = feedRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [run?.pos.done, run?.pos.chars, run?.rows.length, run?.state])

  if (!run) return null
  const shown = run.rows.slice(0, run.pos.done)
  const typingRow =
    run.state === `running` && run.pos.done < run.rows.length && run.pos.chars > 0
      ? run.rows[run.pos.done]
      : null
  const pendingQuestion = run.state === `waiting`
  const lastQuestion = shown.map((r) => r.kind).lastIndexOf(`question`)

  return (
    <div className="ide-session">
      <WorkHeader tabKey={run.tabKey} />
      <div className="ide-feed" ref={feedRef}>
        <div className="ide-workcol ide-feed-col">
          {shown.map((row, i) => (
            <FeedItem
              key={i}
              row={row}
              onAnswer={
                interactive && pendingQuestion && i === lastQuestion ? answerQuestion : undefined
              }
            />
          ))}
          {typingRow && <FeedItem row={typingRow} partial={run.pos.chars} />}
          {run.state === `ended` && (
            <div className="ide-feed-end">{`Stopped · ${RUN_DEVICE} · just now`}</div>
          )}
        </div>
      </div>
      {run.state !== `ended` && !pendingQuestion && <SteerComposer run={run} />}
    </div>
  )
}

export function DiffFace({ tabKey }: { tabKey?: string }) {
  const { interactive } = useIde()
  const run = useTabRun(tabKey)
  const [open, setOpen] = useState<Set<string>>(
    () => new Set(run?.files.filter((f) => f.rows).map((f) => f.path) ?? []),
  )
  if (!run) return null
  return (
    <div className="ide-session">
      <WorkHeader tabKey={run.tabKey} />
      <div className="ide-diffpage">
        <div className="ide-workcol">
          {run.files.map((file) => {
            const expanded = open.has(file.path) && file.rows
            return (
              <div key={file.path} className="ide-filecard">
                <button
                  className={`ide-filecard-head${interactive && file.rows ? ` is-click` : ``}`}
                  type="button"
                  onClick={
                    interactive && file.rows
                      ? () =>
                          setOpen((prev) => {
                            const next = new Set(prev)
                            if (next.has(file.path)) next.delete(file.path)
                            else next.add(file.path)
                            return next
                          })
                      : undefined
                  }
                >
                  <span className="ide-filecard-letter">M</span>
                  <span className="ide-filecard-path">{file.path}</span>
                  <span className="ide-c-green">{`+${file.add}`}</span>
                  <span className="ide-c-red">{`-${file.del}`}</span>
                  {expanded ? (
                    <IcChevDown size={11} className="ide-c-muted" />
                  ) : (
                    <IcChevRight size={11} className="ide-c-muted" />
                  )}
                </button>
                {expanded && (
                  <div className="ide-filecard-body">
                    <DiffView />
                  </div>
                )}
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}
