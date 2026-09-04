/* ─── Bottom terminal dock (terminal_dock.rs `render_strip`, EXP-688): ONE
   29px glass strip, open or collapsed — leading terminal glyph (plus the
   word "Terminal" only when there are no tabs to name it), the session
   chips with the `+` right after them, then the right cluster: "Open in
   new window" for the ACTIVE tab and the open/close chevron. ─── */
import { useEffect, useRef } from "react"
import { SHELL_TAB_TITLE, batchTabTitle, claudeTabTitle, type ScriptLine } from "./data"
import { useIde } from "./state"
import {
  IcArrowUpRight,
  IcChevDown,
  IcChevUp,
  IcPlus,
  IcSquareTerminal,
  IcX,
} from "./icons"

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
    codingTarget,
    codingScript,
    scriptPos,
    interactive,
  } = useIde()
  const termRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    const el = termRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [scriptPos, dockTab, dockOpen, coding])

  /* The strip names itself only while it has no chips to name it. */
  const hasTabs = dockOpen || coding !== `idle`

  const strip = (
    <div
      className={`ide-dock-tabs${interactive ? ` is-click` : ``}`}
      onClick={interactive ? () => setDockOpen(!dockOpen) : undefined}
    >
      <span className="ide-dock-glyph">
        <IcSquareTerminal size={10} />
      </span>
      {hasTabs ? (
        <>
          <button
            className={`ide-dock-tab${dockTab === `shell` ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
            type="button"
            onClick={
              interactive
                ? (e) => {
                    e.stopPropagation()
                    setDockTab(`shell`)
                    setDockOpen(true)
                  }
                : undefined
            }
          >
            {SHELL_TAB_TITLE}
            <span className="ide-dock-x" aria-hidden>
              <IcX size={9} />
            </span>
          </button>
          {coding !== `idle` && (
            <button
              className={`ide-dock-tab${dockTab === `claude` ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
              type="button"
              onClick={
                interactive
                  ? (e) => {
                      e.stopPropagation()
                      setDockTab(`claude`)
                      setDockOpen(true)
                    }
                  : undefined
              }
            >
              {codingTarget?.kind === `batch`
                ? batchTabTitle(codingTarget.issueIds.length)
                : claudeTabTitle(codingTarget?.id ?? ``)}
              {coding === `ended` && <span className="ide-exitbadge">0</span>}
              <span className="ide-dock-x" aria-hidden>
                <IcX size={9} />
              </span>
            </button>
          )}
        </>
      ) : (
        <span className="ide-dock-name">Terminal</span>
      )}
      <span className="ide-icbtn" title="New terminal">
        <IcPlus size={11} />
      </span>
      <div className="ide-flex1" />
      <span className="ide-icbtn" title="Open in new window">
        <IcArrowUpRight size={11} />
      </span>
      <span className="ide-icbtn" title={dockOpen ? `Hide terminal` : `Show terminal`}>
        {dockOpen ? <IcChevDown size={11} /> : <IcChevUp size={11} />}
      </span>
    </div>
  )

  if (!dockOpen) {
    return <div className="ide-dock is-collapsed">{strip}</div>
  }

  const typingLine =
    coding === `running` && scriptPos.done < codingScript.length && scriptPos.chars > 0
      ? codingScript[scriptPos.done]
      : null

  const claudeVisible = dockTab === `claude` && coding !== `idle`

  return (
    <div className="ide-dock">
      {claudeVisible ? (
        <div className="ide-term" ref={termRef}>
          {codingScript.slice(0, scriptPos.done).map((line, i) => (
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
      {/* The strip is pinned to the panel's BOTTOM edge — the dock grows
          upward out of it (EXP-688). */}
      {strip}
    </div>
  )
}
