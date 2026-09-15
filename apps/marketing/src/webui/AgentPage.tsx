/* ─── The Agent page (t/$teamSlug/agent, EXP-818/825/874) ───
   The ONE launcher: suggestion pills over the launch composer (issue chips,
   the prompt field, the `#` / ▶ / `+` tool row and the round submit), the
   muted options line under it (device · agent mark · model · Plan · …), then
   the caller's own runs as filled group BANDS over flat rows — "Running"
   with the EXP-874 unified run row (state dot, identifier + title, the status
   line, circular trailing buttons) and "Recent" (EXP-886, was "Past"), folded
   by default and uncounted (EXP-862). */
import { useState } from "react"
import { getIssue } from "../ide/data"
import { useWeb } from "./state"
import { AgentMark, RunDot, StatusGlyph } from "./bits"
import {
  AGENT_SESSIONS,
  AGENT_SUGGESTIONS,
  PAST_RUNS,
  WEB_DEVICE,
  type AgentSession,
} from "./data"
import {
  ICON_3,
  ICON_35,
  ICON_4,
  IcChevDown,
  IcChevRight,
  IcClose,
  IcDevices,
  IcEllipsis,
  IcHash,
  IcIssue,
  IcMerged,
  IcPlay,
  IcPlus,
  IcSubmit,
} from "./icons"

function RunningRow({ session }: { session: AgentSession }) {
  const { interactive, openIssue } = useWeb()
  const issue = getIssue(session.issueId)
  const review = session.state === `review`
  return (
    <div
      className={`web-srow${interactive ? ` is-click` : ``}`}
      onClick={interactive ? () => openIssue(issue.id, `run`) : undefined}
    >
      <div className="web-srow-main">
        <div className="web-srow-line1">
          <RunDot state={session.state} />
          <span className="web-srow-id">{issue.id}</span>
          <span className="web-srow-title">{issue.title}</span>
        </div>
        <div className={`web-srow-status${review ? ` is-green` : ``}`}>
          {review ? `Ready for review · ${session.device}` : `${session.device} · ${session.started}`}
        </div>
      </div>
      <div className="web-srow-trailing">
        {review && (
          <span className="web-circlebtn" title="Merge">
            <IcMerged size={ICON_4} />
          </span>
        )}
        <button
          type="button"
          className={`web-circlebtn${interactive ? ` is-click` : ``}`}
          title={issue.id}
          onClick={
            interactive
              ? (e) => {
                  e.stopPropagation()
                  openIssue(issue.id, `issue`)
                }
              : undefined
          }
        >
          <IcIssue size={ICON_4} />
        </button>
      </div>
    </div>
  )
}

export function WebAgentPage() {
  const { interactive, agentSeedId } = useWeb()
  const [chips, setChips] = useState<string[]>(agentSeedId ? [agentSeedId] : [])
  const [draft, setDraft] = useState(``)
  const [pastOpen, setPastOpen] = useState(false)

  const submitLabel =
    chips.length > 1 ? `Start batch · ${chips.length}` : chips.length === 1 ? `Start coding` : `Start chat`

  return (
    <div className="web-detail-scroll">
      <div className="web-agentcol">
        <div className="web-suggest">
          {AGENT_SUGGESTIONS.map((s) => (
            <button
              key={s}
              type="button"
              className={`web-suggest-pill${interactive ? ` is-click` : ``}`}
              onClick={interactive ? () => setDraft(s) : undefined}
            >
              {s}
            </button>
          ))}
        </div>

        <div className="web-launch">
          {chips.length > 0 && (
            <div className="web-launch-chips">
              {chips.map((id) => {
                const issue = getIssue(id)
                return (
                  <span className="web-launch-chip" key={id}>
                    <StatusGlyph status={issue.status} size={ICON_3} />
                    <span className="web-srow-id">{issue.id}</span>
                    <span className="web-launch-chiptitle">{issue.title}</span>
                    <button
                      type="button"
                      className={`web-launch-chipx${interactive ? ` is-click` : ``}`}
                      aria-label="Remove"
                      onClick={
                        interactive ? () => setChips((prev) => prev.filter((c) => c !== id)) : undefined
                      }
                    >
                      <IcClose size={ICON_3} />
                    </button>
                  </span>
                )
              })}
            </div>
          )}
          <textarea
            className="web-composer-input"
            placeholder={chips.length ? `Additional instructions (optional)…` : `Ask the agent…`}
            rows={2}
            value={draft}
            readOnly={!interactive}
            onChange={(e) => setDraft(e.target.value)}
          />
          <div className="web-composer-foot">
            <span className="web-editrail-btn" title="Issues">
              <IcHash size={ICON_4} />
            </span>
            <span className="web-editrail-btn" title="Actions">
              <IcPlay size={ICON_4} />
            </span>
            <span className="web-editrail-btn" title="Attach image">
              <IcPlus size={ICON_4} />
            </span>
            <button
              type="button"
              className="web-send"
              disabled={!draft.trim() && chips.length === 0}
              title={submitLabel}
              aria-label={submitLabel}
            >
              <IcSubmit size={27.75} />
            </button>
          </div>
        </div>

        <div className="web-launchopts">
          <span className="web-launchopt">
            <IcDevices size={ICON_35} />
            {WEB_DEVICE}
          </span>
          <span className="web-launchopt">
            <AgentMark agent="claude" />
            <IcChevDown size={ICON_3} />
          </span>
          <span className="web-launchopt">
            Fable
            <IcChevDown size={ICON_3} />
          </span>
          <span className="web-launchopt">
            Default
            <IcChevDown size={ICON_3} />
          </span>
          <span className="web-launchopt">
            Plan
            <span className="web-switch" />
          </span>
          <span className="web-launchopt">
            <IcEllipsis size={ICON_4} />
          </span>
        </div>

        <div className="web-band">Running</div>
        {AGENT_SESSIONS.map((s) => (
          <RunningRow key={s.id} session={s} />
        ))}

        <button
          type="button"
          className={`web-band is-fold${interactive ? ` is-click` : ``}`}
          onClick={interactive ? () => setPastOpen((v) => !v) : undefined}
        >
          <IcChevRight size={ICON_3} className={`web-groupchev-icon${pastOpen ? ` is-open` : ``}`} />
          Recent
        </button>
        {pastOpen &&
          PAST_RUNS.map((run) => (
            <div className="web-srow" key={run.title}>
              <div className="web-srow-main">
                <div className="web-srow-line1">
                  {run.identifier && <span className="web-srow-id">{run.identifier}</span>}
                  <span className="web-srow-title is-plain">{run.title}</span>
                </div>
                <div className="web-srow-status">{run.byline}</div>
              </div>
              <IcChevRight size={ICON_4} className="web-srow-chev" />
            </div>
          ))}
      </div>
    </div>
  )
}
