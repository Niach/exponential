/* ─── HOME, worded "Agent" (chat_screen.rs, EXP-772/825/1019/1294,
   FEED-100): the dawn — the faint top half of the Exponential mark — over
   ONE 640px column: the HEADLINE while there is a subject ("Implement" +
   the issue chips, each with its ✕), THE composer card (the field, the
   "+" menu as its only tool, the round send), and the options line under it
   (Device · Account · Model · Plan). A send starts ONE thread on the device
   and lands on it. ─── */
import { useState, type KeyboardEvent } from "react"
import { RUN_ACCOUNT, RUN_DEVICE, RUN_MODEL, getIssue } from "./data"
import { useIde } from "./state"
import { StatusIcon } from "./bits"
import {
  IcAgentsGlyph,
  IcChevDown,
  IcChevRight,
  IcClaude,
  IcEffort,
  IcIssueRef,
  IcLogo,
  IcMonitor,
  IcPaperclip,
  IcPlus,
  IcSubmit,
  IcUltracode,
  IcX,
} from "./icons"

/* composerUi placeholders (contract.json ×4). */
const HOME_PLACEHOLDER = `What do you want to build?`
const INSTRUCTIONS_PLACEHOLDER = `Additional instructions (optional)…`

/* composer-menu.json rows for a Claude run on a team with agents and no MCP
   servers, on a device without the computer-use capability. */
function PlusMenu({ onClose }: { onClose: () => void }) {
  return (
    <div className="ide-menu is-down" role="menu" onMouseLeave={onClose}>
      <span className="ide-menu-item" role="menuitem">
        <IcIssueRef size={14} />
        Implement issue
        <IcChevRight size={12} className="ide-menu-chev" />
      </span>
      <span className="ide-menu-item" role="menuitem">
        <IcAgentsGlyph size={14} />
        Message an agent
        <IcChevRight size={12} className="ide-menu-chev" />
      </span>
      <span className="ide-menu-item" role="menuitem">
        <IcPaperclip size={14} />
        Add file or image
      </span>
      <span className="ide-menu-sep" />
      <span className="ide-menu-item" role="menuitem">
        <IcEffort size={14} />
        Effort · CLI default
        <IcChevRight size={12} className="ide-menu-chev" />
      </span>
      <span className="ide-menu-item" role="menuitem">
        <IcUltracode size={14} />
        Ultracode
        <span className="ide-switch ide-menu-chev" />
      </span>
    </div>
  )
}

export function ChatScreen() {
  const { interactive, subject, removeSubject, send } = useIde()
  const [draft, setDraft] = useState(``)
  const [menu, setMenu] = useState(false)
  /* The demo scripts issue threads only: without an issue subject the send
     stays disabled (the product would start a plain session). */
  const canSend = subject.length > 0

  const submit = () => {
    if (!canSend) return
    send(draft)
    setDraft(``)
  }
  const onKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === `Enter` && !e.shiftKey) {
      e.preventDefault()
      submit()
    }
  }

  return (
    <div className="ide-home">
      <div className="ide-home-stack">
        {/* FEED-100: the mark's top half, 440px, alpha 0.025 fading out. */}
        <div className="ide-dawn" aria-hidden>
          <IcLogo size={440} />
        </div>
        <div className="ide-home-col">
          {subject.length > 0 && (
            <div className="ide-headline">
              <span className="ide-headline-verb">Implement</span>
              {subject.map((id) => {
                const issue = getIssue(id)
                return (
                  <span key={id} className="ide-issuechip is-removable">
                    <StatusIcon status={issue.status} size={10.5} />
                    <span className="ide-issuechip-id">{issue.id}</span>
                    <span className="ide-issuechip-title">{issue.title}</span>
                    <button
                      className={`ide-issuechip-x${interactive ? ` is-click` : ``}`}
                      type="button"
                      aria-label={`Remove ${issue.id}`}
                      onClick={interactive ? () => removeSubject(id) : undefined}
                    >
                      <IcX size={10.5} />
                    </button>
                  </span>
                )
              })}
            </div>
          )}
          <div className="ide-cmp">
            <textarea
              className="ide-cmp-field"
              rows={2}
              placeholder={subject.length > 0 ? INSTRUCTIONS_PLACEHOLDER : HOME_PLACEHOLDER}
              value={draft}
              readOnly={!interactive}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={interactive ? onKey : undefined}
            />
            <div className="ide-cmp-row">
              <div className="ide-cmp-plus">
                <button
                  className={`ide-cmp-tool${interactive ? ` is-click` : ``}`}
                  type="button"
                  aria-label="Add"
                  aria-expanded={menu}
                  onClick={interactive ? () => setMenu((v) => !v) : undefined}
                >
                  <IcPlus size={12.25} />
                </button>
                {menu && <PlusMenu onClose={() => setMenu(false)} />}
              </div>
              <span className="ide-flex1" />
              <button
                className={`ide-send${interactive ? ` is-click` : ``}`}
                type="button"
                disabled={!canSend}
                aria-label={subject.length > 0 ? `Start coding` : `Start session`}
                title={subject.length > 0 ? `Start coding` : `Start session`}
                onClick={interactive ? submit : undefined}
              >
                <IcSubmit size={16} />
              </button>
            </div>
          </div>
          {/* The options line: inline pins, the muted word + a chevron. */}
          <div className="ide-options">
            <span className="ide-pin">
              <IcMonitor size={12} />
              {RUN_DEVICE}
              <IcChevDown size={12} className="ide-pin-chev" />
            </span>
            <span className="ide-pin">
              <IcClaude size={12} />
              {RUN_ACCOUNT}
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
          </div>
        </div>
      </div>
    </div>
  )
}
