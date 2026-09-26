// variants/LoopReel.tsx — variation "Loop" (EXP-1100 r2): ONE continuous
// shot. A single issue card orbits a ring through four stations — board,
// agent, review, merge — and the camera rides with it: it docks, changes
// state, undocks, leaves a motion trail across the empty stretch, docks
// again. Back at the board it slots in as Done; the camera pulls out to
// reveal the whole closed loop with the mark at its centre.
//
// World space is the orbit's frame (centre 0,0, radius R); the camera is one
// translate+scale on the world layer, so every station is a static piece of
// world and only the card and the camera move.

import React from "react"
import { AbsoluteFill, random } from "remotion"
import { wallpaperBackground } from "../../ships/rig"
import { ExpLogo } from "../../ships/rig"
import { C, DISPLAY, MONO, UI, VIOLET } from "../theme"
import { IN, IN_OUT, POP, SETTLE, SNAP, enter, lerp, mix, pop, seg, stagger, typed } from "../motion"
import { Avatar, Cursor, Glass, GroupBand, Ident, IssueRow, LiveDot, Pill, Ripple, StatusGlyph, type StatusKind } from "../atoms"
import { Decode } from "../atoms-extra"
import { ClaudeMark } from "../../closedloop/surfaces/agentmarks"

const R = 1250
const CARD_W = 760
const CARD_H = 76

// Station windows on the orbit: [arrive, leave]. The card travels 90° in the
// 50 frames between one station's `leave` and the next one's `arrive`.
const STATIONS = [
  { id: `board`, label: `BOARD`, deg: 180, arrive: 0, leave: 58 },
  { id: `agent`, label: `AGENT`, deg: 270, arrive: 108, leave: 160 },
  { id: `review`, label: `REVIEW`, deg: 360, arrive: 210, leave: 262 },
  { id: `merge`, label: `MERGE`, deg: 450, arrive: 312, leave: 362 },
  { id: `home`, label: `BOARD`, deg: 540, arrive: 402, leave: 450 },
] as const

const PULL_AT = 404

const polar = (deg: number, r = R) => {
  const a = (deg * Math.PI) / 180
  return { x: Math.cos(a) * r, y: Math.sin(a) * r }
}

// Orbit angle at frame f (holds at stations, IN_OUT between).
const angleAt = (f: number): number => {
  for (let i = 0; i < STATIONS.length; i++) {
    const s = STATIONS[i]
    if (f < s.arrive) {
      const prev = STATIONS[i - 1]
      return lerp(prev.deg, s.deg, seg(f, prev.leave, s.arrive, IN_OUT))
    }
    if (f <= s.leave) return s.deg
  }
  return STATIONS[STATIONS.length - 1].deg
}

// Which station the card is at (or last left) and the station-local frame.
const stationAt = (f: number) => {
  let idx = 0
  for (let i = 0; i < STATIONS.length; i++) if (f >= STATIONS[i].arrive) idx = i
  return { idx, sf: f - STATIONS[idx].arrive, docked: f <= STATIONS[idx].leave }
}

const travelT = (f: number): number => {
  for (let i = 1; i < STATIONS.length; i++) {
    const a = STATIONS[i - 1].leave
    const b = STATIONS[i].arrive
    if (f > a && f < b) return (f - a) / (b - a)
  }
  return 0
}

// The card's state per station index.
const cardState = (idx: number, sf: number): { status: StatusKind; chip: `run` | `pr` | `merged` | null; assignee: boolean } => {
  if (idx === 0) return { status: `backlog`, chip: null, assignee: sf >= 26 }
  if (idx === 1) return { status: sf >= 14 ? `progress` : `backlog`, chip: sf >= 14 ? `run` : null, assignee: true }
  if (idx === 2) return { status: sf >= 16 ? `review` : `progress`, chip: sf >= 16 ? `pr` : `run`, assignee: true }
  if (idx === 3) return { status: sf >= 30 ? `done` : `review`, chip: sf >= 30 ? `merged` : `pr`, assignee: true }
  return { status: `done`, chip: `merged`, assignee: true }
}

const Card: React.FC<{ state: ReturnType<typeof cardState>; flipPop: number; ghost?: number }> = ({ state, flipPop, ghost = 0 }) => (
  <div
    style={{
      position: `absolute`,
      left: -CARD_W / 2,
      top: -CARD_H / 2,
      width: CARD_W,
      height: CARD_H,
      borderRadius: 16,
      background: `linear-gradient(to bottom, rgba(26,26,30,0.98), rgba(16,16,19,0.98))`,
      border: `1px solid ${state.status === `done` ? `rgba(59,130,246,0.6)` : state.status === `review` ? `rgba(34,197,94,0.5)` : C.strokeActive}`,
      boxShadow: ghost ? `none` : `0 30px 80px -20px rgba(0,0,0,0.85), inset 0 1px 0 rgba(255,255,255,0.08)`,
      display: `flex`,
      alignItems: `center`,
      gap: 16,
      padding: `0 22px`,
      fontFamily: UI,
      fontSize: 26,
      color: C.text,
      opacity: ghost ? 0.35 / ghost : 1,
      filter: ghost ? `blur(${ghost * 2}px)` : undefined,
    }}
  >
    <span style={{ display: `inline-block`, scale: String(1 + 0.4 * (1 - flipPop)) }}>
      <StatusGlyph kind={state.status} size={28} />
    </span>
    <Ident text="EXP-1100" size={21} color={C.muted} />
    <span style={{ flex: 1, fontWeight: 600, letterSpacing: `-0.01em`, whiteSpace: `nowrap`, overflow: `hidden`, textOverflow: `ellipsis` }}>Ship the 15-second showreel</span>
    {state.chip === `run` ? (
      <Pill size={16} fill="rgba(217,119,87,0.14)" stroke="rgba(217,119,87,0.4)">
        <ClaudeMark size={15} /> Running
      </Pill>
    ) : state.chip === `pr` ? (
      <Pill size={16} color={C.statusInReview} fill="rgba(34,197,94,0.12)" stroke="rgba(34,197,94,0.35)">
        <span style={{ fontFamily: MONO }}>#916</span> PR open
      </Pill>
    ) : state.chip === `merged` ? (
      <Pill size={16} color={C.statusDone} fill="rgba(59,130,246,0.14)" stroke="rgba(59,130,246,0.4)">
        Merged
      </Pill>
    ) : null}
    {state.assignee ? <Avatar initials="DS" hue={6} size={32} /> : <span style={{ width: 32, height: 32, borderRadius: `50%`, border: `1.5px dashed ${C.strokeStrong}` }} />}
  </div>
)

// ── stations (world space, each drawn around its orbit point) ──────────────
const BoardStation: React.FC<{ sf: number; docked: boolean; home: boolean }> = ({ sf, docked, home }) => {
  const rows: [string, string, StatusKind][] = home
    ? [
        [`EXP-1097`, `Review wave clears its layer`, `done`],
        [`EXP-1093`, `Issue rail hover opens the mini-graph`, `done`],
        [`EXP-1102`, `Workflow host state lives on the device`, `review`],
      ]
    : [
        [`EXP-1097`, `Review wave clears its layer`, `progress`],
        [`EXP-1093`, `Issue rail hover opens the mini-graph`, `progress`],
        [`EXP-1102`, `Workflow host state lives on the device`, `backlog`],
      ]
  return (
    <Glass x={-470} y={-CARD_H / 2 - 56 - 46 - 4} w={940} h={56 + 46 + 4 + CARD_H + 4 + 3 * 66 + 16} r={20}>
      <div style={{ display: `flex`, alignItems: `center`, gap: 12, height: 56, padding: `0 24px`, borderBottom: `1px solid ${C.strokeSection}`, fontFamily: UI, fontSize: 21, fontWeight: 600, color: C.text }}>
        <span style={{ width: 11, height: 11, borderRadius: 4, background: VIOLET, boxShadow: `0 0 14px ${VIOLET}` }} />
        Product <span style={{ color: C.dim, fontWeight: 500 }}>/ All issues</span>
        <span style={{ flex: 1 }} />
        <LiveDot f={sf} size={9} />
        <span style={{ fontFamily: MONO, fontSize: 14, color: C.muted, letterSpacing: `0.08em` }}>LIVE</span>
      </div>
      <GroupBand label={home ? `Done` : `Backlog`} count={home ? 3 : 4} status={home ? `done` : `backlog`} h={46} font={17} />
      {/* the slot the card docks into */}
      <div style={{ height: CARD_H + 8, background: docked ? `rgba(255,255,255,0.03)` : `transparent`, borderBottom: `1px solid ${C.strokeRow}` }} />
      {rows.map(([id, t, st], i) => (
        <div key={id} style={home ? enter(sf, stagger(6, i, 4), 12, { rise: 0, x: -30, blur: 0 }) : undefined}>
          <IssueRow ident={id} title={t} status={st} prio={(2 - (i % 2)) as 1 | 2} h={66} font={23} glyph={22} assignee={i < 2 ? { initials: [`MK`, `AL`][i], hue: [4, 1][i] } : null} />
        </div>
      ))}
    </Glass>
  )
}

const STREAM = [
  [`Read`, `apps/marketing/src/movie/ships/theme.ts`],
  [`Grep`, `useCurrentFrame · 14 matches`],
  [`Write`, `showreel/variants/LoopReel.tsx`],
  [`Bash`, `bun run typecheck`],
  [`Edit`, `showreel/variants/LoopReel.tsx  +212 −6`],
]

const AgentStation: React.FC<{ sf: number }> = ({ sf }) => (
  <Glass x={-470} y={CARD_H / 2 + 40} w={940} h={360} r={20}>
    <div style={{ display: `flex`, alignItems: `center`, gap: 12, height: 56, padding: `0 24px`, borderBottom: `1px solid ${C.strokeSection}`, fontFamily: UI, fontSize: 19, color: C.text }}>
      <ClaudeMark size={22} />
      <span style={{ fontWeight: 600 }}>claude</span>
      <span style={{ fontFamily: MONO, color: C.dim, fontSize: 15 }}>opus · high</span>
      <span style={{ flex: 1 }} />
      {sf >= 14 ? <LiveDot f={sf} size={9} /> : null}
      <span style={{ fontFamily: MONO, fontSize: 14, color: C.muted, letterSpacing: `0.08em` }}>{sf >= 14 ? `RUNNING` : `IDLE`}</span>
    </div>
    <div style={{ padding: `16px 24px 0`, fontFamily: MONO, fontSize: 21, lineHeight: `44px` }}>
      {STREAM.map(([name, arg], i) => (
        <div key={name + i} style={{ display: `flex`, gap: 12, ...enter(sf, stagger(16, i, 7), 10, { rise: 10, x: -8, blur: 3 }) }}>
          <span style={{ color: C.termToolDot }}>●</span>
          <span style={{ color: C.text, fontWeight: 600 }}>{name}</span>
          <span style={{ color: C.muted }}>{arg}</span>
        </div>
      ))}
      <div style={{ display: `flex`, gap: 12, ...enter(sf, 52, 10, { rise: 10, blur: 3 }) }}>
        <span style={{ display: `inline-block`, color: C.termSpinner, rotate: `${(sf * 14) % 360}deg` }}>✳</span>
        <span style={{ color: C.termSpinner }}>{typed(`Pushing exp/EXP-1100…`, sf, 52, 1.2)}</span>
      </div>
    </div>
  </Glass>
)

const DIFF: [`hunk` | `ctx` | `add` | `del`, string][] = [
  [`hunk`, `@@ -12,6 +12,9 @@ const angleAt`],
  [`ctx`, `  for (const s of STATIONS) {`],
  [`del`, `    if (f < s.arrive) return s.deg`],
  [`add`, `    if (f < s.arrive) {`],
  [`add`, `      return lerp(prev.deg, s.deg, seg(f, prev.leave, s.arrive, IN_OUT))`],
  [`add`, `    }`],
  [`ctx`, `    if (f <= s.leave) return s.deg`],
]

const ReviewStation: React.FC<{ sf: number }> = ({ sf }) => (
  <Glass x={-470} y={CARD_H / 2 + 40} w={940} h={360} r={20}>
    <div style={{ display: `flex`, alignItems: `center`, gap: 12, height: 56, padding: `0 24px`, borderBottom: `1px solid ${C.strokeSection}`, fontFamily: MONO, fontSize: 17, color: C.text }}>
      <span style={{ color: C.muted }}>showreel/variants/</span>
      <span style={{ fontWeight: 600, marginLeft: -12 }}>LoopReel.tsx</span>
      <span style={{ flex: 1 }} />
      <span style={{ color: C.diffAdd }}>+212</span>
      <span style={{ color: C.diffDel }}>−6</span>
      <span style={{ ...enter(sf, 16, 10, { rise: 0, x: 16, blur: 4 }) }}>
        <Pill size={14} color={C.statusInReview} fill="rgba(34,197,94,0.12)" stroke="rgba(34,197,94,0.35)">
          <span style={{ fontFamily: MONO }}>#916</span> open
        </Pill>
      </span>
    </div>
    <div style={{ padding: `12px 0`, fontFamily: MONO, fontSize: 17, lineHeight: `36px` }}>
      {DIFF.map(([t, s], i) => {
        const bg = t === `add` ? C.diffAddBg : t === `del` ? C.diffDelBg : t === `hunk` ? C.hunkBg : `transparent`
        const fg = t === `hunk` ? C.hunkFg : t === `ctx` ? `rgba(250,250,250,0.72)` : C.text
        return (
          <div key={i} style={{ display: `flex`, background: bg, color: fg, ...enter(sf, stagger(6, i, 4), 10, { rise: 0, x: 30, blur: 2 }) }}>
            <span style={{ width: 54, textAlign: `right`, paddingRight: 14, color: C.dim, fontSize: 14 }}>{t === `hunk` ? `` : 12 + i}</span>
            <span style={{ width: 22, color: t === `add` ? C.diffAdd : t === `del` ? C.diffDel : C.dim }}>{t === `add` ? `+` : t === `del` ? `−` : ` `}</span>
            <span style={{ whiteSpace: `pre`, fontStyle: t === `hunk` ? `italic` : undefined }}>{s}</span>
          </div>
        )
      })}
    </div>
  </Glass>
)

const PARTS = Array.from({ length: 40 }, (_, i) => ({
  a: random(`la${i}`) * Math.PI * 2,
  v: 7 + random(`lv${i}`) * 12,
  s: 5 + random(`ls${i}`) * 7,
  c: [C.statusDone, C.statusInReview, VIOLET, C.text][i % 4],
}))

const MergeStation: React.FC<{ sf: number }> = ({ sf }) => {
  const press = seg(sf, 26, 34)
  const merged = sf >= 30
  const cx = mix(sf, 8, 24, 420, 24)
  const cy = mix(sf, 8, 24, 330, 176)
  const burst = sf - 30
  return (
    <>
      <Glass x={-470} y={CARD_H / 2 + 40} w={940} h={260} r={20}>
        <div style={{ display: `flex`, alignItems: `center`, gap: 12, height: 56, padding: `0 24px`, borderBottom: `1px solid ${C.strokeSection}`, fontFamily: UI, fontSize: 19, fontWeight: 600, color: C.text }}>
          Reviews
          <span style={{ color: C.dim, fontWeight: 500 }}>/ EXP-1100</span>
          <span style={{ flex: 1 }} />
          <span style={{ fontFamily: MONO, fontSize: 14, color: merged ? C.statusDone : C.statusInReview, letterSpacing: `0.08em` }}>{merged ? `MERGED` : `CHECKS PASSED`}</span>
        </div>
        <div style={{ display: `flex`, alignItems: `center`, justifyContent: `center`, height: 200, gap: 20 }}>
          {[`typecheck`, `test`, `build`].map((c, i) => (
            <span key={c} style={enter(sf, stagger(4, i, 4), 10, { rise: 10, blur: 4 })}>
              <Pill size={16} color={C.statusInReview} fill="rgba(34,197,94,0.1)" stroke="rgba(34,197,94,0.3)">
                ✓ {c}
              </Pill>
            </span>
          ))}
          <span style={{ width: 24 }} />
          <div
            style={{
              width: 220,
              height: 56,
              borderRadius: 14,
              background: merged ? C.statusDone : C.primary,
              color: merged ? C.text : C.primaryFg,
              display: `flex`,
              alignItems: `center`,
              justifyContent: `center`,
              fontFamily: UI,
              fontSize: 20,
              fontWeight: 600,
              scale: String((1 - 0.08 * Math.sin(press * Math.PI)) * pop(sf, 6, POP)),
              boxShadow: merged ? `0 0 40px rgba(59,130,246,0.6)` : `0 12px 40px -10px rgba(255,255,255,0.35)`,
            }}
          >
            {merged ? `Merged` : `Merge`}
          </div>
        </div>
      </Glass>
      {sf >= 8 ? <Cursor x={cx} y={cy} press={press} /> : null}
      {burst >= 0 && burst < 32
        ? PARTS.map((p, i) => {
            const t = burst
            const x = 24 + Math.cos(p.a) * p.v * t * (1 - t / 80)
            const y = 176 + Math.sin(p.a) * p.v * t * (1 - t / 80) + 0.3 * t * t
            return <div key={i} style={{ position: `absolute`, left: x - p.s / 2, top: y - p.s / 2, width: p.s, height: p.s, borderRadius: i % 3 === 0 ? `50%` : 2, background: p.c, opacity: Math.max(0, 1 - t / 28) }} />
          })
        : null}
    </>
  )
}

// Space dust for parallax across the empty stretches.
const DUST = Array.from({ length: 140 }, (_, i) => ({
  x: (random(`dx${i}`) - 0.5) * 5200,
  y: (random(`dy${i}`) - 0.5) * 5200,
  s: 2 + random(`ds${i}`) * 4,
  o: 0.15 + random(`do${i}`) * 0.4,
  z: random(`dz${i}`) < 0.5 ? 0.55 : 1,
}))

export const LoopReel: React.FC<{ f: number }> = ({ f }) => {
  const deg = angleAt(f)
  const P = polar(deg)
  const tt = travelT(f)
  const { idx, sf, docked } = stationAt(f)
  const state = cardState(idx, sf)

  // Camera: card at screen centre, scale dips mid-travel; the pull-out.
  const pull = seg(f, PULL_AT, PULL_AT + 40)
  const camS = lerp(1 - 0.3 * Math.sin(tt * Math.PI), 0.29, pull)
  const focus = { x: lerp(P.x, 0, pull), y: lerp(P.y, 0, pull) }
  const camX = 960 - focus.x * camS
  const camY = 540 - focus.y * camS

  // Flip pop: the glyph's most recent state change.
  const flipAt = idx === 0 ? 26 : idx === 1 ? 14 : idx === 2 ? 16 : idx === 3 ? 30 : 0
  const flipPop = pop(sf, flipAt, SNAP)

  const moving = tt > 0 && tt < 1
  const ghosts = moving ? [2, 4, 6] : []

  const label = docked ? STATIONS[idx].label : ``
  const ringDraw = seg(f, PULL_AT + 16, PULL_AT + 44)

  return (
    <AbsoluteFill style={{ backgroundColor: C.canvas, overflow: `hidden`, fontFamily: UI, color: C.text }}>
      <AbsoluteFill style={{ backgroundImage: wallpaperBackground(Math.sin(f / 90) * 30, Math.cos(f / 120) * 20), opacity: 0.9 }} />
      <AbsoluteFill style={{ background: `radial-gradient(ellipse 85% 75% at 50% 50%, transparent 45%, rgba(0,0,0,0.65))` }} />

      {/* far dust: parallax at 0.55 */}
      <div style={{ position: `absolute`, left: 0, top: 0, transformOrigin: `0 0`, transform: `translate(${960 - focus.x * camS * 0.55}px, ${540 - focus.y * camS * 0.55}px) scale(${camS})` }}>
        {DUST.filter((d) => d.z < 1).map((d, i) => (
          <div key={i} style={{ position: `absolute`, left: d.x, top: d.y, width: d.s, height: d.s, borderRadius: `50%`, background: `rgba(255,255,255,${d.o})` }} />
        ))}
      </div>

      {/* the world */}
      <div style={{ position: `absolute`, left: 0, top: 0, transformOrigin: `0 0`, transform: `translate(${camX}px, ${camY}px) scale(${camS})` }}>
        {DUST.filter((d) => d.z === 1).map((d, i) => (
          <div key={i} style={{ position: `absolute`, left: d.x, top: d.y, width: d.s, height: d.s, borderRadius: `50%`, background: `rgba(255,255,255,${d.o})` }} />
        ))}
        {/* the orbit + ticks + the closing ring */}
        <svg style={{ position: `absolute`, left: -R - 200, top: -R - 200 }} width={2 * R + 400} height={2 * R + 400} viewBox={`${-R - 200} ${-R - 200} ${2 * R + 400} ${2 * R + 400}`}>
          <circle cx={0} cy={0} r={R} fill="none" stroke={C.strokeActive} strokeWidth={2} strokeDasharray="14 22" />
          {Array.from({ length: 24 }, (_, i) => {
            const a = polar(i * 15, R + 40)
            const b = polar(i * 15, R + (i % 6 === 0 ? 90 : 60))
            return <line key={i} x1={a.x} y1={a.y} x2={b.x} y2={b.y} stroke={C.strokeStrong} strokeWidth={2} />
          })}
          <circle cx={0} cy={0} r={R + 140} fill="none" stroke={C.green} strokeWidth={10} pathLength={1} strokeDasharray={1} strokeDashoffset={1 - ringDraw} strokeLinecap="round" transform="rotate(180)" opacity={0.9} />
        </svg>
        {STATIONS.slice(0, 4).map((s, i) => {
          const p = polar(s.deg)
          const lift = i === 0 ? 250 : 130
          return (
            <div key={s.id} style={{ position: `absolute`, left: p.x - 200, top: p.y - lift - 20, width: 400, textAlign: `center`, fontFamily: MONO, fontSize: 26, letterSpacing: `0.3em`, color: C.dim }}>
              {s.label}
            </div>
          )
        })}

        {/* stations */}
        {STATIONS.map((s, i) => {
          const p = polar(s.deg)
          const local = f - s.arrive
          const near = Math.abs(f - (s.arrive + s.leave) / 2) < 140 || pull > 0
          if (!near) return null
          return (
            <div key={s.id} style={{ position: `absolute`, left: p.x, top: p.y }}>
              {i === 0 || i === 4 ? <BoardStation sf={Math.max(0, local)} docked={idx === i && docked} home={i === 4} /> : null}
              {i === 1 ? <AgentStation sf={local} /> : null}
              {i === 2 ? <ReviewStation sf={local} /> : null}
              {i === 3 ? <MergeStation sf={local} /> : null}
              {/* dock ripple */}
              {idx === i ? <Ripple f={sf} at={0} x={0} y={0} size={520} color={i === 4 ? C.statusDone : C.text} dur={26} /> : null}
            </div>
          )
        })}

        {/* trail + card */}
        {ghosts.map((g) => {
          const gp = polar(angleAt(f - g))
          return (
            <div key={g} style={{ position: `absolute`, left: gp.x, top: gp.y }}>
              <Card state={state} flipPop={1} ghost={g / 2} />
            </div>
          )
        })}
        <div style={{ position: `absolute`, left: P.x, top: P.y, scale: String(1 - 0.06 * Math.sin(tt * Math.PI)) }}>
          <Card state={state} flipPop={flipPop} />
        </div>
      </div>

      {/* screen-space: station label + logo at the loop's centre */}
      <div style={{ position: `absolute`, left: 120, top: 70, fontFamily: MONO, fontSize: 20, letterSpacing: `0.3em`, color: C.muted, opacity: 1 - pull }}>
        {label ? <Decode text={`${String(idx === 4 ? 1 : idx + 1).padStart(2, `0`)} / ${label}`} f={sf} at={2} dur={16} /> : null}
      </div>
      {pull > 0 ? (
        <div style={{ position: `absolute`, left: 0, right: 0, top: 0, bottom: 0, display: `flex`, flexDirection: `column`, alignItems: `center`, justifyContent: `center`, gap: 22, opacity: seg(f, PULL_AT + 14, PULL_AT + 30) }}>
          <div style={{ width: 130, height: 130, scale: String(0.6 + 0.4 * pop(f, PULL_AT + 14, SETTLE)), filter: `drop-shadow(0 0 40px rgba(255,255,255,0.2))` }}>
            <ExpLogo size={130} drawT={seg(f, PULL_AT + 16, PULL_AT + 36)} />
          </div>
          <div style={{ fontFamily: DISPLAY, fontSize: 64, fontWeight: 600, letterSpacing: `-0.03em`, ...enter(f, PULL_AT + 24, 14, { rise: 20, blur: 10 }) }}>Exponential</div>
          <div style={{ fontFamily: MONO, fontSize: 22, letterSpacing: `0.06em`, color: C.muted, ...enter(f, PULL_AT + 30, 12, { rise: 12, blur: 6 }) }}>exponential.at · the closed loop</div>
        </div>
      ) : null}
      <div style={{ position: `absolute`, left: 0, bottom: 0, height: 3, width: (f / 450) * 1920, background: `linear-gradient(to right, ${VIOLET}, ${C.green})`, opacity: 0.75 }} />
      {/* a hair of vignette on the exit */}
      <AbsoluteFill style={{ backgroundColor: `#000`, opacity: seg(f, 446, 450, IN) * 0 }} />
    </AbsoluteFill>
  )
}
