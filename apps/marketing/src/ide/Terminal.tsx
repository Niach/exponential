/* ─── Bottom terminal dock: 29px collapsed strip, session tabs, typed claude script ─── */
import { useEffect, useRef } from "react"
import {
  CLAUDE_TAB_TITLE,
  CODING_SCRIPT,
  SHELL_TAB_TITLE,
  type ScriptLine,
} from "./data"
import { useIde } from "./state"
import { IcChevDown, IcChevUp, IcPlus, IcSquareTerminal, IcX } from "./icons"

function TermLine({ line, partial }: { line: ScriptLine; partial?: number }) {
  const text = partial === undefined ? line.text : line.text.slice(0, partial)
  return (
    <div className="ide-term-line">
      {line.kind === `cmd` && <span className="ide-term-prompt">{`$ `}</span>}
      {line.kind === `ok` && <span className="ide-term-ok">{`✓ `}</span>}
      {line.kind === `claude` && <span className="ide-term-claude">{`● `}</span>}
      <span className={line.kind === `cmd` ? `ide-term-cmd` : `ide-term-out`}>{text}</span>
      {partial !== undefined && <span className="ide-caret" />}
    </div>
  )
}

export function TerminalDock() {
  const {
    dockOpen,
    setDockOpen,
    dockTab,
    setDockTab,
    coding,
    scriptPos,
    interactive,
  } = useIde()
  const termRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    const el = termRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [scriptPos, dockTab, dockOpen, coding])

  const tabCount = coding === `idle` ? 1 : 2

  if (!dockOpen) {
    return (
      <button
        className={`ide-dock-strip${interactive ? ` is-click` : ``}`}
        type="button"
        onClick={interactive ? () => setDockOpen(true) : undefined}
      >
        <IcSquareTerminal size={14} />
        <span>{`Terminal (${tabCount})`}</span>
        <div className="ide-flex1" />
        <IcChevUp size={14} />
      </button>
    )
  }

  const typingLine =
    coding === `running` && scriptPos.done < CODING_SCRIPT.length && scriptPos.chars > 0
      ? CODING_SCRIPT[scriptPos.done]
      : null

  const claudeVisible = dockTab === `claude` && coding !== `idle`

  return (
    <div className="ide-dock">
      <div className="ide-dock-tabs">
        <button
          className={`ide-dock-tab${dockTab === `shell` ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
          type="button"
          onClick={interactive ? () => setDockTab(`shell`) : undefined}
        >
          <span className="ide-dock-star">✳</span>
          {SHELL_TAB_TITLE}
          <span className="ide-dock-x" aria-hidden>
            <IcX size={10} />
          </span>
        </button>
        {coding !== `idle` && (
          <button
            className={`ide-dock-tab${dockTab === `claude` ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
            type="button"
            onClick={interactive ? () => setDockTab(`claude`) : undefined}
          >
            <span className="ide-dock-star">✳</span>
            {CLAUDE_TAB_TITLE}
            {coding === `ended` && <span className="ide-exitbadge">0</span>}
            <span className="ide-dock-x" aria-hidden>
              <IcX size={10} />
            </span>
          </button>
        )}
        <div className="ide-flex1" />
        <button className="ide-ghost ide-icbtn" type="button" title="New shell">
          <IcPlus size={12} />
        </button>
        <button
          className={`ide-ghost ide-icbtn${interactive ? ` is-click` : ``}`}
          type="button"
          title="Hide terminal"
          onClick={interactive ? () => setDockOpen(false) : undefined}
        >
          <IcChevDown size={14} />
        </button>
      </div>
      {claudeVisible ? (
        <div className="ide-term" ref={termRef}>
          {CODING_SCRIPT.slice(0, scriptPos.done).map((line, i) => (
            <TermLine key={i} line={line} />
          ))}
          {typingLine && <TermLine line={typingLine} partial={scriptPos.chars} />}
        </div>
      ) : (
        <div className="ide-term" ref={termRef}>
          <div className="ide-term-line">
            <span className="ide-term-ok">{`❯ `}</span>
            <span className="ide-caret" />
          </div>
        </div>
      )}
      {claudeVisible && coding === `ended` && (
        <div className="ide-dock-status">
          <span className="ide-exitdot" />
          Process finished with exit code 0
        </div>
      )}
    </div>
  )
}
