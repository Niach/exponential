/* ─── The Agent page (chat_screen.rs, EXP-825) — the ONE launcher on every
   client, and the surface that RETIRED the three-tab Start-coding dialog
   (EXP-845 dropped its mock from this site). A full-width center screen with
   one wide composer card, vertically centred:

     [EXP-8 ✕] [EXP-12 ✕]        ← the subject chips (OR one action chip)
     the prompt field (@ members · # issues · : emoji)
     #  ▶  🖼                              ( Start batch · 2 )
     Device ▾ · Agent ▾ · Model ▾ · Plan ○ · ⋯

   Subject by SWAP: issue chips or one action chip, never both, nothing
   disabled. Free text with no subject is a chat prompt, beside a subject it
   is additional instructions. The submit pill carries the contract label
   (Start chat / Start coding / Start batch · N / Run action); the muted
   options line under the card is the same launch model every surface uses.
   Suggestion chips show over the EMPTY, subject-less field (EXP-820). ─── */
import { useState } from "react"
import { getIssue } from "./data"
import { StatusIcon } from "./bits"
import { useIde } from "./state"
import {
  IcChevDown,
  IcEllipsis,
  IcHash,
  IcImageConcept,
  IcPlay,
  IcX,
} from "./icons"

/* A few of `lib/chat-suggestions.ts` CHAT_SUGGESTION_POOL — the real chips,
   fixed here so the page renders the same on every load (the product draws
   them at random per mount). */
const SUGGESTIONS = [
  `Fix #`,
  `Label every issue in the backlog`,
  `Find duplicate issues and link them`,
  `Which issues are blocked, and by what?`,
] as const

/* The pickers on the muted options line (launch_options.rs). Values cycle on
   click, like the dialog's rows used to. */
const DEVICES = [`Danny's MacBook Pro`, `build-box — Mira Chen`] as const
const AGENTS = [`Claude Code`, `Codex`] as const
const MODELS = [`Fable`, `Opus`, `Sonnet`] as const

function InlinePicker({
  label,
  value,
  onCycle,
}: {
  label: string
  value: string
  onCycle?: () => void
}) {
  const { interactive } = useIde()
  return (
    <button
      className={`ide-cmp-pick${interactive && onCycle ? ` is-click` : ``}`}
      type="button"
      title={label}
      onClick={interactive ? onCycle : undefined}
    >
      <span>{value}</span>
      <IcChevDown size={10} className="ide-c-muted" />
    </button>
  )
}

export function ChatScreen() {
  const { chips, toggleChip, submitComposer, interactive } = useIde()
  const [text, setText] = useState(``)
  const [device, setDevice] = useState(0)
  const [agent, setAgent] = useState(0)
  const [model, setModel] = useState(0)
  const [plan, setPlan] = useState(true)

  /* contract `launchSubmit` ×4: the label names what the send starts. */
  const submitLabel =
    chips.length > 1
      ? `Start batch · ${chips.length}`
      : chips.length === 1
        ? `Start coding`
        : `Start chat`
  const blocked = chips.length === 0 && text.trim().length === 0

  return (
    <div className="ide-chat">
      <div className="ide-chat-col">
        {chips.length === 0 && text.length === 0 && (
          <div className="ide-cmp-suggestions">
            {SUGGESTIONS.map((suggestion) => (
              <button
                key={suggestion}
                className={`ide-cmp-chip is-suggestion${interactive ? ` is-click` : ``}`}
                type="button"
                onClick={interactive ? () => setText(suggestion) : undefined}
              >
                {suggestion}
              </button>
            ))}
          </div>
        )}
        <div className="ide-cmp-card">
          {chips.length > 0 && (
            <div className="ide-cmp-chips">
              {chips.map((id) => {
                const issue = getIssue(id)
                return (
                  <span key={id} className="ide-cmp-chip">
                    <StatusIcon status={issue.status} size={10} />
                    <span className="ide-cmp-chip-id">{issue.id}</span>
                    <span className="ide-cmp-chip-title">{issue.title}</span>
                    <button
                      className={`ide-cmp-chip-x${interactive ? ` is-click` : ``}`}
                      type="button"
                      aria-label={`Remove ${issue.id}`}
                      onClick={interactive ? () => toggleChip(id) : undefined}
                    >
                      <IcX size={9} />
                    </button>
                  </span>
                )
              })}
            </div>
          )}
          <textarea
            className="ide-cmp-field"
            placeholder={
              chips.length > 0
                ? `Additional instructions (optional)…`
                : `Ask the agent…`
            }
            value={text}
            readOnly={!interactive}
            onChange={(e) => setText(e.target.value)}
          />
          <div className="ide-cmp-tools">
            {/* `#` opens the issue picker, ▶ the action picker, the image
                glyph the file chooser — the three tools, every client. */}
            <span className="ide-cmp-tool" title="Issues">
              <IcHash size={13} />
            </span>
            <span className="ide-cmp-tool" title="Actions">
              <IcPlay size={13} />
            </span>
            <span className="ide-cmp-tool" title="Attach image">
              <IcImageConcept size={13} />
            </span>
            <div className="ide-flex1" />
            <button
              className={`ide-btn-primary ide-cmp-submit${interactive && !blocked ? ` is-click` : ``}`}
              type="button"
              disabled={blocked}
              onClick={interactive && !blocked ? submitComposer : undefined}
            >
              {submitLabel}
            </button>
          </div>
        </div>
        {/* Options row B (EXP-825): one muted line, no loose controls. */}
        <div className="ide-cmp-options">
          <InlinePicker
            label="Device"
            value={DEVICES[device]}
            onCycle={() => setDevice((i) => (i + 1) % DEVICES.length)}
          />
          <InlinePicker
            label="Agent"
            value={AGENTS[agent]}
            onCycle={() => setAgent((i) => (i + 1) % AGENTS.length)}
          />
          <InlinePicker
            label="Model"
            value={MODELS[model]}
            onCycle={() => setModel((i) => (i + 1) % MODELS.length)}
          />
          <span className="ide-cmp-toggle">
            <span>Plan</span>
            <button
              className={`ide-switch${plan ? ` is-on` : ``}${interactive ? ` is-click` : ``}`}
              type="button"
              role="switch"
              aria-checked={plan}
              aria-label="Plan mode"
              onClick={interactive ? () => setPlan((v) => !v) : undefined}
            >
              <span className="ide-switch-knob" />
            </button>
          </span>
          {/* `⋯` unfolds Effort, Ultracode, MCP servers and Account. */}
          <span className="ide-cmp-more" title="More options">
            <IcEllipsis size={12} />
          </span>
        </div>
      </div>
    </div>
  )
}
