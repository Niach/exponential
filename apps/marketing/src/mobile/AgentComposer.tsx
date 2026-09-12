/* ─── Mobile Agent page — faithful phone recreation (EXP-825/845) ───
   The ONE launcher on every client, and what REPLACED the three-tab
   Start-coding sheet this file used to recreate: a pushed "Agent" detail
   whose `AgentComposerCard` (ExpUI `GlassComposer`) carries the subject
   chips, the prompt field and a tool row — `#` issues, ▶ actions, the image
   glyph — with the LABELLED primary submit pill ("Start coding" / "Start
   batch · N" / "Start chat" / "Run action"). Under the card sits the muted
   options row of glass pills (Device · Agent · Model · Plan · ⋯,
   `AgentOptionsRow`), then the caller's own sessions (`AgentSessionsList`).

   EVERY number below is authored in iOS POINTS: `.mag-screen` is a 414pt-wide
   canvas that `mobile.css` scales down with one transform, so the recreation
   stays measurable against the store shot instead of drifting into
   hand-tuned marketing px. Decorative only (rendered under aria-hidden +
   inert). */
import type { ReactNode } from "react"
import { AGENTS } from "../components/agent-icons"
import { IcChev, IcClose, IcHash, IcImage, IcMore, IcPlay } from "../components/icons"
import { ISSUES } from "../ide/data"
import { MagStatusIcon } from "./sheet-icons"

/* The issue the play button chipped — the same board fixture the IDE demo
   codes on (a single chip = a single run, two or more = a batch). */
const CHIPPED = ISSUES.find((issue) => issue.id === `EXP-8`)!

/* AgentSessionsList: the caller's own runs under the composer. */
const PAST: { id: string; caption: string }[] = [
  { id: `EXP-11`, caption: `In review · PR #214 · 12 min ago` },
  { id: `EXP-5`, caption: `Merged · PR #212 · 2h ago` },
  { id: `EXP-7`, caption: `Ended · Claude Code · yesterday` },
]

const { Logo: ClaudeLogo } = AGENTS[0]

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
        {/* The pushed detail's inline nav bar — native back, no tab bar. */}
        <div className={`mag-nav`}>
          <span className={`mag-back`}>
            <IcChev size={17} stroke={2.2} />
          </span>
          <span className={`mag-navtitle`}>Agent</span>
        </div>

        {/* GlassComposer: leading chips · field · tools + submit. */}
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
              <IcHash size={17} stroke={2} />
            </span>
            <span className={`mag-tool`}>
              <IcPlay size={17} stroke={2} />
            </span>
            <span className={`mag-tool`}>
              <IcImage size={17} stroke={2} />
            </span>
            <span className={`mag-spacer`} />
            {/* GlassPill, primary: the submit is LABELLED on mobile. */}
            <span className={`mag-submit`}>Start coding</span>
          </div>
        </div>

        {/* AgentOptionsRow: one muted line of glass pills. The phone's row
            SCROLLS — Model and the rest sit behind the `⋯` here. */}
        <div className={`mag-options`}>
          <OptionPill>MacBook Pro</OptionPill>
          <OptionPill>
            <ClaudeLogo size={13} />
            Claude Code
          </OptionPill>
          <span className={`mag-pill is-toggle`}>
            Plan
            <span className={`mag-switch`} />
          </span>
          <OptionPill chevron={false}>
            <IcMore size={14} stroke={2} />
          </OptionPill>
        </div>

        {/* The caller's runs — Running is empty until this one starts. */}
        <div className={`mag-sectionlabel`}>Past</div>
        <div className={`mag-card is-list`}>
          {PAST.map(({ id, caption }) => {
            const issue = ISSUES.find((row) => row.id === id)!
            return (
              <div key={id} className={`mag-srow`}>
                <ClaudeLogo size={15} />
                <span className={`mag-srow-main`}>
                  <span className={`mag-srow-title`}>{issue.title}</span>
                  <span className={`mag-srow-caption`}>{caption}</span>
                </span>
                <span className={`mag-chipid`}>{id}</span>
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}
