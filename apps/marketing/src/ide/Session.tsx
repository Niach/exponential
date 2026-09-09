/* ─── The session screen (session_screen.rs, EXP-746/773/787/791) — a coding
   run is a FULL-WIDTH center screen, never a panel beside a list and never a
   terminal: the header (status dot · mono identifier · subject · the phase
   caption with the machine · Kill session), the ACP transcript in its 736px
   reading column, the collapsible "Latest changes" bar and the one composer.
   EXP-773 deleted the PTY transcript this demo used to type out; what streams
   now is the agent's own narration, its tool calls and the cards you answer.
   The bottom session bar (session_bar.rs) is NOT drawn: it carries terminal
   tabs alone since EXP-791 and takes no height while none is open. ─── */
import { useEffect, useRef } from "react"
import { BATCH_RUN_TITLE, DIFF_FILE, getIssue, type FeedRow } from "./data"
import { useIde } from "./state"
import {
  IcChevRight,
  IcCircleArrowUp,
  IcCircleQuestion,
  IcCircleStop,
  IcFile,
  IcPlus,
  IcSparkles,
  IcWrench,
} from "./icons"

/* steer::feed rows. Prose takes the body rung (14/22), everything else the
   tool rung (12/18); the glyph rides the reading column's left edge. */
function FeedItem({ row, partial }: { row: FeedRow; partial?: number }) {
  if (row.kind === `narration`) {
    const text = partial === undefined ? row.text : row.text.slice(0, partial)
    return (
      <div className="ide-feed-row is-prose">
        <IcSparkles size={10.5} className="ide-feed-glyph" />
        <div className="ide-feed-text">
          {text}
          {partial !== undefined && <span className="ide-caret" />}
        </div>
      </div>
    )
  }
  if (row.kind === `tool`) {
    return (
      <div className="ide-feed-row is-tool">
        <IcWrench size={10.5} className="ide-feed-glyph" />
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
        <IcChevRight size={10.5} className="ide-feed-chev" />
        <IcWrench size={10.5} className="ide-feed-glyph" />
        <span className="ide-feed-caption">{row.caption}</span>
      </div>
    )
  }
  return (
    <div className="ide-answercard">
      <IcCircleQuestion size={10.5} className="ide-answercard-glyph" />
      <div className="ide-answercard-text">{row.text}</div>
      {row.options.map((option, i) => (
        /* EXP-788: numbered option buttons — keys 1…9 and Enter pick them,
           and typing in the composer answers the card just as well. */
        <div className="ide-answeropt" key={option.title}>
          <span className="ide-answeropt-key">{i + 1}</span>
          <span className="ide-answeropt-body">
            <span className="ide-answeropt-title">{option.title}</span>
            <span className="ide-answeropt-sub">{option.sub}</span>
          </span>
        </div>
      ))}
    </div>
  )
}

export function SessionScreen() {
  const { coding, codingTarget, codingScript, scriptPos, stopCoding, interactive } =
    useIde()
  const feedRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    const el = feedRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [scriptPos, coding])

  if (!codingTarget) return null
  const issue = codingTarget.kind === `issue` ? getIssue(codingTarget.id) : null
  const typingRow =
    coding === `running` && scriptPos.done < codingScript.length && scriptPos.chars > 0
      ? codingScript[scriptPos.done]
      : null
  const edited = codingScript
    .slice(0, scriptPos.done)
    .some((row) => row.kind === `tool` && row.verb === `Edit`)
  /* `header_status`: the phase caption names the machine, and the dot takes
     its tone — green while the agent works, amber while it waits. */
  const caption =
    coding === `waiting`
      ? `Needs your input · Danny's MacBook Pro`
      : coding === `ended`
        ? `Ended by you · Claude Code · Danny's MacBook Pro · just now`
        : `Working · Danny's MacBook Pro`

  return (
    <div className="ide-session">
      <div className="ide-session-head">
        <span
          className={`ide-session-dot${coding === `waiting` ? ` is-waiting` : coding === `ended` ? ` is-ended` : ``}`}
        />
        {issue && <span className="ide-session-id">{issue.id}</span>}
        <span className="ide-session-subject">
          {issue ? issue.title : BATCH_RUN_TITLE}
        </span>
        <span className="ide-session-caption">{caption}</span>
        {coding !== `ended` && (
          <button
            className={`ide-icbtn${interactive ? ` is-click` : ``}`}
            type="button"
            title="Kill session"
            onClick={interactive ? stopCoding : undefined}
          >
            <IcCircleStop size={11} />
          </button>
        )}
      </div>
      <div className="ide-feed" ref={feedRef}>
        <div className="ide-feed-col">
          {codingScript.slice(0, scriptPos.done).map((row, i) => (
            <FeedItem key={i} row={row} />
          ))}
          {typingRow && <FeedItem row={typingRow} partial={scriptPos.chars} />}
        </div>
      </div>
      {/* session_extras::changes_bar — the branch's diff against the base of
          `origin/<default>`, collapsed. It appears with the first edit. */}
      {edited && (
        <div className="ide-changes">
          <IcChevRight size={10.5} className="ide-feed-chev" />
          <IcFile size={10.5} className="ide-feed-glyph" />
          <span className="ide-changes-label">Latest changes</span>
          <span className="ide-changes-add">{`+${DIFF_FILE.add}`}</span>
          <span className="ide-changes-del">{`−${DIFF_FILE.del}`}</span>
        </div>
      )}
      {coding !== `ended` && (
        <div className="ide-steer">
          <span className="ide-steer-placeholder">
            {coding === `waiting`
              ? `Answer directly, or pick an option above`
              : `Message the agent… (/ for commands)`}
          </span>
          <span className="ide-steer-tool">
            <IcPlus size={11} />
          </span>
          <span className="ide-steer-send">
            <IcCircleArrowUp size={16} />
          </span>
        </div>
      )}
    </div>
  )
}
