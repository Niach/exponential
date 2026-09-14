/* ─── Mobile Agent page — faithful phone recreation (EXP-825/827/874/877) ───
   The ONE launcher on every client: a pushed "Agent" detail (round glass back
   button, centred title) whose `AgentComposerCard` (ExpUI `GlassComposer`)
   carries the subject chips, the prompt field and a tool row — `#` issues,
   ▶ actions, the `+` attach — with the round send GLYPH on the right
   (EXP-827: icon-only ×4; the contract's "Start coding" is only its
   accessibility name). Under the card sits the options row of glass pills
   (Device · the agent's orange mark · Model · Plan, `AgentOptionsRow`), then
   the caller's own runs (`AgentSessionsList`): filled group BANDS over flat
   rows (EXP-818) — "Running" with the EXP-874 unified `RunningSessionRow`
   (state dot, identifier + title, the status line, circular trailing
   buttons) and "Past", folded by default with its count (EXP-862).

   EVERY number below is authored in iOS POINTS: `.mag-screen` is a 414pt-wide
   canvas that `mobile.css` scales down with one transform, so the recreation
   stays measurable against the store shot instead of drifting into
   hand-tuned marketing px. Decorative only (rendered under aria-hidden +
   inert). */
import type { ReactNode } from "react"
import { CircleArrowUp, FileText, GitMerge, Monitor } from "lucide-react"
import { ClaudeLogo } from "../components/agent-icons"
import { IcChev, IcClose, IcHash, IcPlay, IcPlus } from "../components/icons"
import { ISSUES } from "../ide/data"
import { MagStatusIcon } from "./sheet-icons"

/* The issue the play button chipped — the same board fixture the IDE demo
   codes on (a single chip = a single run, two or more = a batch). */
const CHIPPED = ISSUES.find((issue) => issue.id === `EXP-8`)!

/* The caller's live run — its PR is open, so the row offers Merge. */
const RUNNING = ISSUES.find((issue) => issue.id === `EXP-11`)!

/* The folded Past band's count (AgentSessionsList.pastSection). */
const PAST_COUNT = 3

/* Anthropic's brand orange — the Claude mark is untinted on every client
   (EXP-877). */
const CLAUDE_FILL = `#D97757`

function OptionPill({
  children,
  chevron = true,
}: {
  children: ReactNode
  chevron?: boolean
}) {
  return (
    <span className={`mag-pill`}>
      {children}
      {chevron && <IcChev size={11} stroke={2} className={`mag-pillchev`} />}
    </span>
  )
}

export function MobileAgentComposer() {
  return (
    <div className={`mag-phone`}>
      <div className={`mag-screen`}>
        {/* The pushed detail's nav bar — the round glass back button and the
            centred title; no tab bar on a detail. */}
        <div className={`mag-nav`}>
          <span className={`mag-back`}>
            <IcChev size={19} stroke={2.2} />
          </span>
          <span className={`mag-navtitle`}>Agent</span>
        </div>

        {/* GlassComposer: leading chips · field · tools + round submit. */}
        <div className={`mag-card`}>
          <div className={`mag-chips`}>
            <span className={`mag-chip`}>
              <MagStatusIcon status={CHIPPED.status} />
              <span className={`mag-chipid`}>{CHIPPED.id}</span>
              <span className={`mag-chiptitle`}>{CHIPPED.title}</span>
              <span className={`mag-chipx`}>
                <IcClose size={11} stroke={2} />
              </span>
            </span>
          </div>
          <div className={`mag-field`}>Additional instructions (optional)…</div>
          <div className={`mag-tools`}>
            <span className={`mag-tool`}>
              <IcHash size={19} stroke={2} />
            </span>
            <span className={`mag-tool`}>
              <IcPlay size={19} stroke={2} />
            </span>
            <span className={`mag-tool`}>
              <IcPlus size={19} stroke={2} />
            </span>
            <span className={`mag-spacer`} />
            {/* GlassComposerSubmitButton: the round `ui-submit` glyph, lit
                because the chip makes the start submittable. */}
            <span className={`mag-submit`} aria-label={`Start coding`}>
              <CircleArrowUp size={30} strokeWidth={1.8} />
            </span>
          </div>
        </div>

        {/* AgentOptionsRow: one line of glass pills. The phone's row SCROLLS,
            so the tail sits past the right edge. */}
        <div className={`mag-options`}>
          <OptionPill chevron={false}>
            <Monitor size={14} strokeWidth={2} />
            MacBook Pro
          </OptionPill>
          <OptionPill>
            <span className={`mag-mark`} style={{ color: CLAUDE_FILL }}>
              <ClaudeLogo size={15} />
            </span>
          </OptionPill>
          <OptionPill>Fable</OptionPill>
          <span className={`mag-pill is-toggle`}>
            Plan
            <span className={`mag-switch`} />
          </span>
        </div>

        {/* The caller's runs: GlassSectionBand + flat RunningSessionRow. */}
        <div className={`mag-band`}>Running</div>
        <div className={`mag-srow`}>
          <span className={`mag-srow-main`}>
            <span className={`mag-srow-line1`}>
              <span className={`mag-dot`} />
              <span className={`mag-chipid`}>{RUNNING.id}</span>
              <span className={`mag-srow-title`}>{RUNNING.title}</span>
            </span>
            <span className={`mag-srow-caption`}>
              <span className={`mag-srow-state`}>Ready for review</span>
              {` · MacBook Pro`}
            </span>
          </span>
          <span className={`mag-circle`}>
            <GitMerge size={18} strokeWidth={2} />
          </span>
          <span className={`mag-circle`}>
            <FileText size={18} strokeWidth={2} />
          </span>
        </div>

        <div className={`mag-band is-fold`}>
          Past
          <span className={`mag-band-count`}>
            {PAST_COUNT}
            <IcChev size={12} stroke={2} className={`mag-pillchev`} />
          </span>
        </div>
      </div>
    </div>
  )
}
