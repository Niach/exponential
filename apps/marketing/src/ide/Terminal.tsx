/* ─── Bottom terminal dock (terminal_dock.rs, EXP-688/723/742): ONE 29px
   strip pinned to the panel's bottom edge, open or collapsed — the rich-tab
   chips with the `+` right after them, and NOTHING else (an empty strip
   names itself: the terminal glyph + "Terminal"). The OPEN dock grows
   upward out of it on the opaque popover, topped by its own 28px header
   row: "Open in new window", the collapsed-form switch (strip ⇄ bubble,
   EXP-742) and "Hide terminal", right-aligned. ─── */
import { useEffect, useRef } from "react"
import { SHELL_TAB_TITLE, batchTabTitle, getIssue, type ScriptLine } from "./data"
import { useIde } from "./state"
import {
  IcArrowUpRight,
  IcChevDown,
  IcMessageCircle,
  IcPlus,
  IcSquareTerminal,
  IcTerminal,
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
  const issue = codingTarget?.kind === `issue` ? getIssue(codingTarget.id) : null

  const strip = (
    <div
      className={`ide-dock-tabs${interactive ? ` is-click` : ``}`}
      onClick={interactive ? () => setDockOpen(!dockOpen) : undefined}
    >
      {hasTabs ? (
        <>
          {/* surface::rich_tab — the shell chip leads with the
              `session-shell` glyph and is named by its cwd. */}
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
            <IcTerminal size={10.5} />
            <span className="ide-dock-title">{SHELL_TAB_TITLE}</span>
            <span className="ide-dock-x" aria-hidden>
              <IcX size={9} />
            </span>
          </button>
          {coding !== `idle` && (
            /* A session chip: the status dot, the mono identifier and the
               issue title (a batch run names itself). */
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
              <span className="ide-dock-dot" />
              {issue ? (
                <>
                  <span className="ide-dock-id">{issue.id}</span>
                  <span className="ide-dock-title">{issue.title}</span>
                </>
              ) : (
                <span className="ide-dock-title">
                  {batchTabTitle(codingTarget?.kind === `batch` ? codingTarget.issueIds.length : 0)}
                </span>
              )}
              {coding === `ended` && <span className="ide-exitbadge">0</span>}
              <span className="ide-dock-x" aria-hidden>
                <IcX size={9} />
              </span>
            </button>
          )}
        </>
      ) : (
        <>
          <span className="ide-dock-glyph">
            <IcSquareTerminal size={10.5} />
          </span>
          <span className="ide-dock-name">Terminal</span>
        </>
      )}
      <span className="ide-icbtn" title="New terminal">
        <IcPlus size={11} />
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
      {/* terminal_dock::render_dock_header (EXP-723): the window controls
          that used to sit on the strip. The middle glyph is EXP-742's
          two-way switch — "Collapse to a bubble" / "Collapse to the strip". */}
      <div className="ide-dock-head">
        <span className="ide-icbtn" title="Open in new window">
          <IcArrowUpRight size={11} />
        </span>
        <span className="ide-icbtn" title="Collapse to a bubble">
          <IcMessageCircle size={11} />
        </span>
        <button
          className={`ide-icbtn${interactive ? ` is-click` : ``}`}
          type="button"
          title="Hide terminal"
          onClick={interactive ? () => setDockOpen(false) : undefined}
        >
          <IcChevDown size={11} />
        </button>
      </div>
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
