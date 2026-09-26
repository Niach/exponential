// acts/Board.tsx — the live board (2.2–5.4 s): a glass panel settles, group
// bands and rows cascade in, the hero issue's title types itself, a teammate
// picks it up, a run starts on it (the play chip), another issue flips to
// review as its PR opens. The camera drifts in, then punches through the
// panel into the agents act.

import React from "react"
import { AbsoluteFill } from "remotion"
import { C, MONO, UI, VIOLET } from "../theme"
import { IN, POP, SNAP, enter, mix, pop, seg, stagger, typed } from "../motion"
import {
  Avatar,
  Caption,
  Glass,
  GroupBand,
  Ident,
  IssueRow,
  LiveDot,
  Pill,
  Ripple,
  StatusGlyph,
  type Person,
  type StatusKind,
} from "../atoms"
import { ClaudeMark } from "../../closedloop/surfaces/agentmarks"

const PX = 260
const PY = 118
const PW = 1400
const PH = 612
const HEADER = 64
const ROW = 62
const BAND = 42

const DS: Person = { initials: `DS`, hue: 6 }
const MK: Person = { initials: `MK`, hue: 4 }
const AL: Person = { initials: `AL`, hue: 1 }
const JR: Person = { initials: `JR`, hue: 3 }

const HERO_TITLE = `Ship the 15-second showreel`

type Row = {
  ident: string
  title: string
  status: StatusKind
  prio: 0 | 1 | 2 | 3
  assignee: Person | null
}

const STARTED: Row[] = [
  { ident: `EXP-1100`, title: HERO_TITLE, status: `progress`, prio: 3, assignee: null },
  { ident: `EXP-1097`, title: `Review wave clears its layer before landing`, status: `progress`, prio: 2, assignee: MK },
  { ident: `EXP-1093`, title: `Issue rail hover opens the mini-graph`, status: `progress`, prio: 1, assignee: AL },
]

const BACKLOG: Row[] = [
  { ident: `EXP-1102`, title: `Workflow host state lives on the device`, status: `backlog`, prio: 2, assignee: JR },
  { ident: `EXP-1104`, title: `Android: unbroken tee on tree connectors`, status: `backlog`, prio: 1, assignee: null },
  { ident: `EXP-1105`, title: `Digest picks the reader's local hour`, status: `backlog`, prio: 1, assignee: null },
  { ident: `EXP-1106`, title: `Passkey login through the desktop handoff`, status: `backlog`, prio: 0, assignee: null },
]

const TYPE_AT = 16
const ASSIGN_AT = 50
const RUN_AT = 58
const REVIEW_AT = 72

export const Board: React.FC<{ l: number }> = ({ l }) => {
  // Camera: settle in, then punch through.
  const camS = mix(l, 0, 90, 1.07, 1) * mix(l, 84, 96, 1, 1.4, IN)
  const camO = 1 - seg(l, 86, 95, IN)
  const camBlur = seg(l, 84, 96, IN) * 14
  const panel = enter(l, 0, 16, { rise: 30, blur: 12 })

  const heroTyped = typed(HERO_TITLE, l, TYPE_AT, 1.1)
  const caretOn = l >= TYPE_AT - 4 && l < TYPE_AT + 34 && l % 14 < 8
  const heroRowY = PY + HEADER + BAND + ROW / 2
  const reviewRowY = PY + HEADER + BAND + ROW * 1.5
  const glyphX = PX + 24 + 11

  const assignPop = pop(l, ASSIGN_AT, POP)
  const runPop = pop(l, RUN_AT, SNAP)
  const prPop = pop(l, REVIEW_AT + 2, POP)
  const secondStatus: StatusKind = l >= REVIEW_AT ? `review` : `progress`

  const presence = [DS, MK, AL]

  return (
    <AbsoluteFill
      style={{
        opacity: camO,
        scale: String(camS),
        filter: camBlur > 0.2 ? `blur(${camBlur}px)` : undefined,
      }}
    >
      <div style={{ position: `absolute`, inset: 0, ...panel }}>
        <Glass x={PX} y={PY} w={PW} h={PH} r={18}>
          {/* header */}
          <div
            style={{
              display: `flex`,
              alignItems: `center`,
              gap: 14,
              height: HEADER,
              padding: `0 26px`,
              borderBottom: `1px solid ${C.strokeSection}`,
              fontFamily: UI,
              fontSize: 22,
              fontWeight: 600,
              color: C.text,
            }}
          >
            <span
              style={{
                width: 12,
                height: 12,
                borderRadius: 4,
                background: VIOLET,
                boxShadow: `0 0 16px ${VIOLET}`,
              }}
            />
            Product
            <span style={{ color: C.dim, fontWeight: 500 }}>/ All issues</span>
            <span style={{ flex: 1 }} />
            <span style={{ display: `flex`, alignItems: `center`, gap: 10, ...enter(l, 26, 14, { rise: 0, x: 24 }) }}>
              <LiveDot f={l} size={10} />
              <span style={{ fontFamily: MONO, fontSize: 15, color: C.muted, letterSpacing: `0.06em` }}>LIVE</span>
            </span>
            <span style={{ display: `flex`, marginLeft: 6 }}>
              {presence.map((p, i) => (
                <span
                  key={p.initials}
                  style={{
                    marginLeft: i === 0 ? 0 : -8,
                    scale: String(pop(l, stagger(28, i, 4), POP)),
                    borderRadius: `50%`,
                    boxShadow: `0 0 0 2px #0f0f12`,
                  }}
                >
                  <Avatar initials={p.initials} hue={p.hue} size={30} />
                </span>
              ))}
            </span>
          </div>

          <div style={enter(l, 8, 12, { rise: 0, x: -30, blur: 4 })}>
            <GroupBand label="In progress" count={3} status="progress" h={BAND} />
          </div>
          {STARTED.map((row, i) => {
            const at = stagger(10, i, 4)
            const isHero = i === 0
            return (
              <div key={row.ident} style={enter(l, at, 14, { rise: 0, x: -44, blur: 6 })}>
                <IssueRow
                  ident={row.ident}
                  status={i === 1 ? secondStatus : row.status}
                  prio={row.prio}
                  h={ROW}
                  assignee={isHero ? (assignPop > 0.01 ? DS : null) : row.assignee}
                  title={
                    isHero ? (
                      <>
                        {heroTyped}
                        {caretOn ? (
                          <span style={{ display: `inline-block`, width: 2, height: `1em`, background: C.text, verticalAlign: `-0.15em`, marginLeft: 1 }} />
                        ) : null}
                      </>
                    ) : (
                      row.title
                    )
                  }
                  trailing={
                    isHero ? (
                      <span style={{ display: `inline-flex`, gap: 8 }}>
                        <span style={{ scale: String(pop(l, TYPE_AT + 30, POP)), opacity: l >= TYPE_AT + 30 ? 1 : 0 }}>
                          <Pill size={15} color={C.muted}>via widget</Pill>
                        </span>
                        <span style={{ scale: String(runPop), opacity: runPop > 0.01 ? 1 : 0, transformOrigin: `right center` }}>
                          <Pill size={15} fill="rgba(217,119,87,0.14)" stroke="rgba(217,119,87,0.4)">
                            <ClaudeMark size={14} />
                            Auto-started
                            <LiveDot f={l} size={7} />
                          </Pill>
                        </span>
                      </span>
                    ) : i === 1 ? (
                      <span style={{ scale: String(prPop), opacity: prPop > 0.01 ? 1 : 0 }}>
                        <Pill size={15} color={C.statusInReview} fill="rgba(34,197,94,0.12)" stroke="rgba(34,197,94,0.35)">
                          <Ident text="#912" size={14} color={C.statusInReview} />
                          PR open
                        </Pill>
                      </span>
                    ) : undefined
                  }
                />
              </div>
            )
          })}

          <div style={enter(l, 22, 12, { rise: 0, x: -30, blur: 4 })}>
            <GroupBand label="Backlog" count={4} status="backlog" h={BAND} />
          </div>
          {BACKLOG.map((row, i) => (
            <div key={row.ident} style={enter(l, stagger(24, i, 4), 14, { rise: 0, x: -44, blur: 6 })}>
              <IssueRow ident={row.ident} title={row.title} status={row.status} prio={row.prio} h={ROW} assignee={row.assignee} />
            </div>
          ))}
        </Glass>

        {/* the assignee landing on the hero row */}
        <Ripple f={l} at={ASSIGN_AT} x={PX + PW - 24 - 13} y={heroRowY} size={70} color={C.text} />
        {/* the run starting */}
        <Ripple f={l} at={RUN_AT} x={glyphX} y={heroRowY} size={90} color={C.statusInProgress} />
        {/* the second issue flipping to review */}
        <Ripple f={l} at={REVIEW_AT} x={glyphX} y={reviewRowY} size={110} color={C.statusInReview} />
        {l >= REVIEW_AT && l < REVIEW_AT + 10 ? (
          <div style={{ position: `absolute`, left: glyphX - 11, top: reviewRowY - 11, scale: String(1 + (1 - seg(l, REVIEW_AT, REVIEW_AT + 10)) * 0.8) }}>
            <StatusGlyph kind="review" size={22} />
          </div>
        ) : null}
      </div>

      <Caption f={l} at={20} out={82} index="01 / Intake" line="Feedback files itself. Runs start on their own." />
    </AbsoluteFill>
  )
}
