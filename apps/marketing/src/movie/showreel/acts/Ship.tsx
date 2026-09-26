// acts/Ship.tsx — review & merge (10.9–13.9 s): a four-node workflow DAG
// draws itself (edges on, nodes in wave order), every node goes to review
// as its PR opens, the cursor lands on Merge stack, and a blue front sweeps
// out from the button flipping nodes to Done as it reaches them, with a
// burst of status-coloured particles. "Shipped." lands on top.

import React from "react"
import { AbsoluteFill, random } from "remotion"
import { C, DISPLAY, MONO, UI, VIOLET } from "../theme"
import { IN, POP, SETTLE, enter, mix, pop, seg } from "../motion"
import { Caption, Cursor, Ident, Pill, Ripple, StatusGlyph, type StatusKind } from "../atoms"

type Node = { id: string; title: string; x: number; y: number; at: number; reviewAt: number; pr: string }

const NW = 320
const NH = 70

const NODES: Node[] = [
  { id: `EXP-1097`, title: `Rail geometry`, x: 520, y: 540, at: 2, reviewAt: 34, pr: `#913` },
  { id: `EXP-1098`, title: `Hover mini-graph`, x: 960, y: 390, at: 10, reviewAt: 40, pr: `#914` },
  { id: `EXP-1099`, title: `Tree connectors`, x: 960, y: 690, at: 13, reviewAt: 44, pr: `#915` },
  { id: `EXP-1100`, title: `Ship the showreel`, x: 1400, y: 540, at: 20, reviewAt: 50, pr: `#916` },
]

const EDGES: [number, number, number][] = [
  [0, 1, 8],
  [0, 2, 10],
  [1, 3, 18],
  [2, 3, 20],
]

const BTN = { x: 1400, y: 880, w: 236, h: 58 }
const PRESS_AT = 56
const SWEEP_AT = 58
const EXIT_AT = 80

const edgePath = (a: Node, b: Node) => {
  const x0 = a.x + NW / 2
  const x1 = b.x - NW / 2
  const dx = (x1 - x0) * 0.5
  return `M ${x0} ${a.y} C ${x0 + dx} ${a.y}, ${x1 - dx} ${b.y}, ${x1} ${b.y}`
}

const dist = (x: number, y: number) => Math.hypot(x - (BTN.x), y - BTN.y)

const COLORS = [C.statusDone, C.statusInReview, VIOLET, C.text]
const PARTICLES = Array.from({ length: 44 }, (_, i) => ({
  a: random(`a${i}`) * Math.PI * 2,
  v: 7 + random(`v${i}`) * 12,
  s: 5 + random(`s${i}`) * 7,
  c: COLORS[i % COLORS.length],
  spin: (random(`r${i}`) - 0.5) * 40,
}))

export const Ship: React.FC<{ l: number }> = ({ l }) => {
  const ex = seg(l, EXIT_AT, EXIT_AT + 10, IN)
  const sweepFrame = l - SWEEP_AT
  const doneAt = (n: Node) => SWEEP_AT + dist(n.x, n.y) / 34

  // cursor
  const cx = mix(l, 40, 54, 1760, BTN.x + 30)
  const cy = mix(l, 40, 54, 1040, BTN.y + 12)
  const press = seg(l, PRESS_AT, PRESS_AT + 10)
  const btnScale = 1 - 0.08 * Math.sin(press * Math.PI)

  const shipped = pop(l, 66, SETTLE)

  return (
    <AbsoluteFill
      style={{
        opacity: 1 - ex,
        scale: String(1 - 0.08 * ex),
        filter: ex > 0.02 ? `blur(${14 * ex}px)` : undefined,
      }}
    >
      {/* edges */}
      <svg style={{ position: `absolute`, inset: 0 }} width={1920} height={1080}>
        {EDGES.map(([ai, bi, at]) => {
          const a = NODES[ai]
          const b = NODES[bi]
          const draw = seg(l, at, at + 16)
          const merged = l >= doneAt(b)
          return (
            <g key={`${ai}-${bi}`}>
              <path d={edgePath(a, b)} fill="none" stroke={C.strokeActive} strokeWidth={2} pathLength={1} strokeDasharray={1} strokeDashoffset={1 - draw} />
              <path d={edgePath(a, b)} fill="none" stroke={merged ? C.statusDone : C.statusInReview} strokeWidth={2} pathLength={1} strokeDasharray={1} strokeDashoffset={1 - seg(l, b.reviewAt, b.reviewAt + 12)} opacity={0.9} />
            </g>
          )
        })}
      </svg>

      {/* the front: an expanding ring from the button */}
      {sweepFrame >= 0 ? (
        <div
          style={{
            position: `absolute`,
            left: BTN.x - sweepFrame * 34,
            top: BTN.y - sweepFrame * 34,
            width: sweepFrame * 68,
            height: sweepFrame * 68,
            borderRadius: `50%`,
            border: `3px solid ${C.statusDone}`,
            opacity: Math.max(0, 1 - sweepFrame / 34) * 0.7,
            boxShadow: `0 0 60px rgba(59,130,246,0.35)`,
          }}
        />
      ) : null}

      {/* nodes */}
      {NODES.map((n) => {
        const s = pop(l, n.at, SETTLE)
        const dAt = doneAt(n)
        const kind: StatusKind = l >= dAt ? `done` : l >= n.reviewAt ? `review` : `progress`
        const flipScale = 1 + 0.35 * (1 - seg(l, l >= dAt ? dAt : n.reviewAt, (l >= dAt ? dAt : n.reviewAt) + 10))
        const prPop = pop(l, n.reviewAt + 2, POP)
        const glow = l >= dAt ? `0 0 0 1px rgba(59,130,246,0.6), 0 0 50px rgba(59,130,246,0.35)` : l >= n.reviewAt ? `0 0 0 1px rgba(34,197,94,0.45), 0 0 40px rgba(34,197,94,0.18)` : `0 20px 60px -20px rgba(0,0,0,0.7)`
        return (
          <div
            key={n.id}
            style={{
              position: `absolute`,
              left: n.x - NW / 2,
              top: n.y - NH / 2,
              width: NW,
              height: NH,
              borderRadius: 16,
              background: `linear-gradient(to bottom, rgba(24,24,28,0.98), rgba(14,14,17,0.98))`,
              border: `1px solid ${C.strokeCard}`,
              boxShadow: glow,
              display: `flex`,
              alignItems: `center`,
              gap: 12,
              padding: `0 18px`,
              opacity: Math.min(1, s * 1.5),
              scale: String(0.6 + 0.4 * s),
              fontFamily: UI,
              color: C.text,
            }}
          >
            <span style={{ display: `inline-block`, scale: String(flipScale) }}>
              <StatusGlyph kind={kind} size={24} />
            </span>
            <span style={{ display: `flex`, flexDirection: `column`, minWidth: 0, flex: 1 }}>
              <Ident text={n.id} size={15} color={C.muted} />
              <span style={{ fontSize: 19, fontWeight: 600, letterSpacing: `-0.01em`, whiteSpace: `nowrap`, overflow: `hidden`, textOverflow: `ellipsis` }}>{n.title}</span>
            </span>
            <span style={{ scale: String(prPop), opacity: prPop > 0.01 ? 1 : 0, transformOrigin: `right center` }}>
              {kind === `done` ? (
                <Pill size={13} color={C.statusDone} fill="rgba(59,130,246,0.14)" stroke="rgba(59,130,246,0.4)">
                  Merged
                </Pill>
              ) : (
                <Pill size={13} color={C.statusInReview} fill="rgba(34,197,94,0.12)" stroke="rgba(34,197,94,0.35)">
                  <span style={{ fontFamily: MONO }}>{n.pr}</span>
                </Pill>
              )}
            </span>
          </div>
        )
      })}
      {NODES.map((n) => (
        <React.Fragment key={`r-${n.id}`}>
          <Ripple f={l} at={n.reviewAt} x={n.x - NW / 2 + 30} y={n.y} size={70} color={C.statusInReview} />
          <Ripple f={l} at={Math.round(doneAt(n))} x={n.x - NW / 2 + 30} y={n.y} size={120} color={C.statusDone} width={3} />
        </React.Fragment>
      ))}

      {/* Merge stack */}
      <div
        style={{
          position: `absolute`,
          left: BTN.x - BTN.w / 2,
          top: BTN.y - BTN.h / 2,
          width: BTN.w,
          height: BTN.h,
          borderRadius: 14,
          background: l >= SWEEP_AT ? C.statusDone : C.primary,
          color: l >= SWEEP_AT ? C.text : C.primaryFg,
          display: `flex`,
          alignItems: `center`,
          justifyContent: `center`,
          gap: 10,
          fontFamily: UI,
          fontSize: 20,
          fontWeight: 600,
          letterSpacing: `-0.01em`,
          scale: String(btnScale * pop(l, 30, POP)),
          boxShadow: l >= SWEEP_AT ? `0 0 40px rgba(59,130,246,0.6)` : `0 12px 40px -10px rgba(255,255,255,0.35)`,
          ...enter(l, 30, 12, { rise: 16, blur: 6 }),
        }}
      >
        <svg width={20} height={20} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.2} strokeLinecap="round" strokeLinejoin="round">
          <circle cx="18" cy="18" r="3" />
          <circle cx="6" cy="6" r="3" />
          <path d="M6 21V9a9 9 0 0 0 9 9" />
        </svg>
        {l >= SWEEP_AT ? `Merged` : `Merge stack`}
      </div>

      {/* particles */}
      {sweepFrame >= 0 && sweepFrame < 34
        ? PARTICLES.map((p, i) => {
            const t = sweepFrame
            const x = BTN.x + Math.cos(p.a) * p.v * t * (1 - t / 80)
            const y = BTN.y + Math.sin(p.a) * p.v * t * (1 - t / 80) + 0.32 * t * t
            return (
              <div
                key={i}
                style={{
                  position: `absolute`,
                  left: x - p.s / 2,
                  top: y - p.s / 2,
                  width: p.s,
                  height: p.s,
                  borderRadius: i % 3 === 0 ? `50%` : 2,
                  background: p.c,
                  opacity: Math.max(0, 1 - t / 30),
                  rotate: `${p.spin * t}deg`,
                }}
              />
            )
          })
        : null}

      {l >= 40 && l < EXIT_AT ? <Cursor x={cx} y={cy} press={press} /> : null}

      {/* the word */}
      <div
        style={{
          position: `absolute`,
          left: 0,
          right: 0,
          top: 130,
          textAlign: `center`,
          fontFamily: DISPLAY,
          fontSize: 136,
          fontWeight: 600,
          letterSpacing: `-0.045em`,
          color: C.text,
          opacity: Math.min(1, shipped * 1.6),
          scale: String(1.25 - 0.25 * shipped),
          filter: shipped < 0.95 ? `blur(${(1 - shipped) * 18}px)` : undefined,
          textShadow: `0 0 80px rgba(59,130,246,0.35)`,
        }}
      >
        Shipped.
      </div>

      <Caption f={l} at={22} out={76} index="04 / Ship" line="Review, merge, done. In one place." />
    </AbsoluteFill>
  )
}
