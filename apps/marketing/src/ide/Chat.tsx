/* ─── The Agent page (chat_screen.rs, EXP-825/874) — the ONE launcher on
   every client: the suggestion chips over the EMPTY field, one wide composer
   card (subject chips, the prompt field, the # · ▶ · + tools and the round
   submit whose tooltip is the contract label: Start chat / Start coding /
   Start batch · N), the muted options line (Device · the agent's brand mark ·
   Model · Plan · ⋯), then the Running band and the folded Past band. ─── */
import { useState } from "react"
import { getIssue, PAST_RUNS, REVIEWS, RUN_DEVICE, type AgentKind } from "./data"
import { StatusIcon } from "./bits"
import { isLive, useIde, type RunView } from "./state"
import { AgentMark } from "./Topbar"
import {
  IcChevDown,
  IcChevRight,
  IcCircleArrowUp,
  IcEllipsis,
  IcFile,
  IcGitMerge,
  IcHash,
  IcMonitor,
  IcPlay,
  IcPlus,
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
const DEVICES = [RUN_DEVICE, `build-box — Mira Chen`] as const
const AGENTS: AgentKind[] = [`claude`, `codex`]
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
              <IcPlus size={13} />
            </span>
            <div className="ide-flex1" />
            <button
              className={`ide-cmp-submit${interactive && !blocked ? ` is-click` : ``}`}
              type="button"
              title={submitLabel}
              aria-label={submitLabel}
              disabled={blocked}
              onClick={interactive && !blocked ? submitComposer : undefined}
            >
              <IcCircleArrowUp size={20} />
            </button>
          </div>
        </div>
        {/* Options row B (EXP-825): one muted line, no loose controls. */}
        <div className="ide-cmp-options">
          <IcMonitor size={12} className="ide-c-muted" />
          <InlinePicker
            label="Device"
            value={DEVICES[device]}
            onCycle={() => setDevice((i) => (i + 1) % DEVICES.length)}
          />
          {/* The agent picker is the brand mark in a pill (EXP-877). */}
          <button
            className={`ide-cmp-agent${interactive ? ` is-click` : ``}`}
            type="button"
            title="Agent"
            onClick={interactive ? () => setAgent((i) => (i + 1) % AGENTS.length) : undefined}
          >
            <AgentMark agent={AGENTS[agent]} size={13} />
            <IcChevDown size={10} className="ide-c-muted" />
          </button>
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
        <RunBands />
      </div>
    </div>
  )
}

/* ─── run_rows.rs (EXP-874): ONE layout per kind, no agent brand marks. ───
   Running: status dot · identifier · title, the agent caption, a toned
   status line, trailing circle buttons (Merge while the PR is open, Open
   issue). Past: identifier · title over the device · time byline, a
   chevron. */
function agentCaption(run: RunView): string | null {
  if (run.state !== `running`) return null
  const last = run.rows.slice(0, run.pos.done).filter((r) => r.kind === `tool`).pop()
  return last && last.kind === `tool` ? `${last.verb === `Edit` ? `Editing` : `Reading`} ${last.target.split(`/`).pop()}` : `Thinking…`
}

function statusLine(run: RunView): { text: string; tone: string } {
  switch (run.state) {
    case `waiting`:
      return { text: `Needs input · ${RUN_DEVICE}`, tone: `is-amber` }
    case `review`:
      return { text: `Ready for review · ${RUN_DEVICE}`, tone: `is-green` }
    default:
      return { text: `${RUN_DEVICE} · started ${run.id === `review` ? `2 hours ago` : `just now`}`, tone: `` }
  }
}

function RunningRow({ run }: { run: RunView }) {
  const { interactive, openRun, openIssue, mergeReview, goneReviews } = useIde()
  const [armed, setArmed] = useState(false)
  const caption = agentCaption(run)
  const status = statusLine(run)
  const prKey = run.issueId ?? `batch`
  const canMerge =
    run.state === `review` &&
    (run.issueId ? REVIEWS.some((r) => r.issueId === run.issueId) : true) &&
    !goneReviews.has(prKey)
  return (
    <div
      className={`ide-runrow${interactive ? ` is-click` : ``}`}
      onClick={interactive ? () => openRun(run.id) : undefined}
    >
      <div className="ide-runrow-main">
        <div className="ide-runrow-line1">
          <span
            className={`ide-rundot${run.state === `waiting` ? ` is-waiting` : ``}`}
          />
          {run.issueId && <span className="ide-runrow-id">{run.issueId}</span>}
          <span className="ide-runrow-title">{run.title}</span>
        </div>
        {caption && <div className="ide-runrow-caption">{caption}</div>}
        <div className={`ide-runrow-status ${status.tone}`}>{status.text}</div>
      </div>
      {canMerge && (
        <button
          className={`ide-circlebtn${armed ? ` is-armed` : ``}${interactive ? ` is-click` : ``}`}
          type="button"
          title={armed ? `Confirm merge` : `Merge`}
          onClick={
            interactive
              ? (e) => {
                  e.stopPropagation()
                  if (armed) mergeReview(prKey)
                  setArmed(!armed)
                }
              : undefined
          }
        >
          <IcGitMerge size={12} />
        </button>
      )}
      {run.issueId && (
        <button
          className={`ide-circlebtn${interactive ? ` is-click` : ``}`}
          type="button"
          title="Open issue"
          onClick={
            interactive
              ? (e) => {
                  e.stopPropagation()
                  if (run.issueId) openIssue(run.issueId)
                }
              : undefined
          }
        >
          <IcFile size={12} />
        </button>
      )}
    </div>
  )
}

function RunBands() {
  const { runs, interactive, openRun, openIssue } = useIde()
  const [pastOpen, setPastOpen] = useState(false)
  const running = runs.filter((r) => isLive(r.state))
  const endedRuns = runs.filter((r) => !isLive(r.state))
  const pastCount = endedRuns.length + PAST_RUNS.length
  return (
    <div className="ide-runbands">
      {running.length > 0 && (
        <>
          <div className="ide-band">Running</div>
          {running.map((run) => (
            <RunningRow key={run.id} run={run} />
          ))}
        </>
      )}
      <button
        className={`ide-band is-fold${interactive ? ` is-click` : ``}`}
        type="button"
        onClick={interactive ? () => setPastOpen((v) => !v) : undefined}
      >
        {pastOpen ? <IcChevDown size={11} /> : <IcChevRight size={11} />}
        <span className="ide-flex1">Past</span>
        <span className="ide-band-count">{pastCount}</span>
      </button>
      {pastOpen && (
        <>
          {endedRuns.map((run) => (
            <PastRow
              key={run.id}
              identifier={run.issueId}
              title={run.title}
              byline={`${RUN_DEVICE} · just now`}
              onOpen={interactive ? () => openRun(run.id) : undefined}
            />
          ))}
          {PAST_RUNS.map((past) => {
            const issue = getIssue(past.issueId)
            return (
              <PastRow
                key={past.issueId}
                identifier={issue.id}
                title={issue.title}
                byline={`${RUN_DEVICE} · ${past.ended}`}
                onOpen={interactive ? () => openIssue(issue.id) : undefined}
              />
            )
          })}
        </>
      )}
    </div>
  )
}

function PastRow({
  identifier,
  title,
  byline,
  onOpen,
}: {
  identifier: string | null
  title: string
  byline: string
  onOpen?: () => void
}) {
  return (
    <div className={`ide-runrow${onOpen ? ` is-click` : ``}`} onClick={onOpen}>
      <div className="ide-runrow-main">
        <div className="ide-runrow-line1">
          {identifier && <span className="ide-runrow-id">{identifier}</span>}
          <span className="ide-runrow-title">{title}</span>
        </div>
        <div className="ide-runrow-status">{byline}</div>
      </div>
      <IcChevRight size={12} className="ide-c-muted" />
    </div>
  )
}
