// surfaces/flowgraph.tsx — FlowGraph: the S10 hero branch graph (center pane).
// Visual language from ref/desktop-source-control-diff.png's LEFT source-control
// tree (the "master ✓" row, nested release row with green state dot + "worktree"
// tag, further-nested EXP rows hanging off 1px vertical connector rails) — staged
// BIG: 34px rows, 16px mono labels, plus horizontal flow lanes that fork from
// exp/rel-v0-12 and merge back with green sweeps, pulse rings at the junctions,
// and a POP "⑂ PR #219 · open" chip at the end of the release lane.
// The component fills its parent (position:absolute inset 0) — the assembler
// places it over the center pane (window-local x 304–1568, below the tab strip;
// 1264×673 while the dock is expanded). All coordinates are stage-local px.

import React from "react"
import { interpolate, interpolateColors, spring } from "remotion"
import { C, EASE, MONO_FONT, POP, UI_FONT } from "../theme"
import { LANES, RELEASE } from "../fixtures"
import { riseIn } from "../rig"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

// Eased interpolate shorthand (always clamped).
const ez = (frame: number, from: number, to: number, a: number, b: number) =>
  interpolate(frame, [from, to], [a, b], { ...CLAMP, easing: EASE })

// POP spring gated so pre-start renders the resting state (0).
const pop = (frame: number, at: number) => (frame < at ? 0 : spring({ frame: frame - at, fps: 30, config: POP }))

// theme hex → rgba (hues stay single-sourced from theme.ts).
const alpha = (hex: string, a: number) => {
  const n = parseInt(hex.slice(1), 16)
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${a})`
}

const NEVER = 1e9
const pick = (arr: ReadonlyArray<number>, i: number): number => arr[i] ?? arr[arr.length - 1] ?? NEVER

// ── Stage geometry (34px rows, 16px mono labels — storyboard §2.7) ────────────
const ROW_H = 34
const IND = 44 // indent per tree depth
const LANE_X0 = 420 // uniform left edge of the horizontal flow lanes
const MERGE_X = [600, 700, 800, 900, 980] as const // junction x per EXP lane (fixture order)
const REL_END = 1020 // release lane right edge (PR chip hangs after it)
const MAIN_END = 1180
const WORK_GAP = 40 // straight run between working lane end and the merge S-curve
const UP_COUNTS = ["↑2", "↑1", "↑3", "↑2 ↓1", "↑1"] as const // trailing muted ahead-counts
// PR-state "open work" dot — storyboard §2.7 pins #FACC15 (== C.synNumber).
const DOT_WORK = C.synNumber

// ── Tiny inline icons (lucide-style, currentColor) ────────────────────────────
const CheckIcon: React.FC<{ size?: number }> = ({ size = 15 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round">
    <path d="M20 6 9 17l-5-5" />
  </svg>
)

const PrIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <circle cx="6" cy="6" r="3" />
    <circle cx="18" cy="18" r="3" />
    <path d="M13 6h3a2 2 0 0 1 2 2v7" />
    <line x1="6" y1="9" x2="6" y2="21" />
  </svg>
)

// Dash-offset revealed path.
const Draw: React.FC<{ d: string; len: number; drawn: number; stroke: string; o?: number; w?: number }> = ({
  d,
  len,
  drawn,
  stroke,
  o = 1,
  w = 1.5,
}) =>
  drawn <= 0.5 ? null : (
    <path
      d={d}
      stroke={stroke}
      strokeOpacity={o}
      strokeWidth={w}
      fill="none"
      strokeLinecap="round"
      strokeDasharray={len}
      strokeDashoffset={Math.max(0, len - drawn)}
    />
  )

// ── Schedule (all values COMPOSITION-GLOBAL frames) ───────────────────────────
export type FlowGraphSchedule = {
  drawMain: number // main row + main lane draw start (24f)
  drawRel: number // rel connector + row (14f), rel lane draw (+8, 24f)
  wave1At: number[] // fork starts for exp/EXP-139/141/143 (fixture order)
  wave1MergeAt: number[] // merge pulses; dots flip at mergeAt−8, sweep draws [−10, 0]
  wave2At: number[] // fork starts for exp/EXP-144/145
  wave2MergeAt: number[]
  prChipAt: number // rel lane brightens + "⑂ PR #219 · open" chip POP
  prMergedAt?: number // optional (S11 auto-ship): chip flips to "merged" (indigo)
}

export type FlowGraphProps = {
  frame: number
  schedule: FlowGraphSchedule
  width?: number // stage size — defaults to the dock-expanded center pane
  height?: number
  padLeft?: number
  padTop?: number
}

export const FlowGraph: React.FC<FlowGraphProps> = ({
  frame,
  schedule: s,
  width = 1264,
  height = 673,
  padLeft = 72,
  padTop = 150,
}) => {
  const rowY = (i: number) => padTop + i * ROW_H + ROW_H / 2
  const mainY = rowY(0)
  const relY = rowY(1)
  const relDotX = padLeft + IND
  const relLabelX = relDotX + 16
  const expDotX = padLeft + 2 * IND
  const expLabelX = expDotX + 16
  const railX = relDotX // depth-2 rail drops from under the rel lane's dot

  // Per-EXP lane data (fixture order groups wave 1 before wave 2).
  const expLanes = LANES.filter((l) => l.depth === 2)
  let w1 = 0
  let w2 = 0
  const exps = expLanes.map((l, k) => {
    const isW1 = l.wave !== 2
    const wi = isW1 ? w1++ : w2++
    const forkAt = pick(isW1 ? s.wave1At : s.wave2At, wi)
    const mergeAt = pick(isW1 ? s.wave1MergeAt : s.wave2MergeAt, wi)
    const y = rowY(2 + k)
    return {
      name: l.name,
      worktree: l.worktree === true,
      y,
      forkAt,
      mergeAt,
      mx: MERGE_X[k] ?? REL_END - WORK_GAP,
      dy: y - relY,
      up: UP_COUNTS[k] ?? `↑1`,
    }
  })

  // main → rel tree connector (rounded elbow; exact length for the dash draw).
  const cx0 = padLeft + 7
  const connD = `M ${cx0} ${mainY + 13} L ${cx0} ${relY - 6} A 6 6 0 0 0 ${cx0 + 6} ${relY} L ${relDotX - 9} ${relY}`
  const connLen = relY - 6 - (mainY + 13) + (Math.PI / 2) * 6 + (relDotX - 9 - (cx0 + 6))
  const connDrawn = ez(frame, s.drawRel, s.drawRel + 14, 0, connLen)

  // Shared depth-2 vertical rail: its bottom edge extends to each row as it forks.
  let railBottom = relY + 9
  for (const e of exps) railBottom = Math.max(railBottom, ez(frame, e.forkAt, e.forkAt + 10, relY + 9, e.y))

  // Horizontal flow lanes.
  const mainLen = MAIN_END - LANE_X0
  const mainDrawn = ez(frame, s.drawMain, s.drawMain + 24, 0, mainLen)
  const relLen = REL_END - LANE_X0
  const relDrawn = ez(frame, s.drawRel + 8, s.drawRel + 32, 0, relLen)
  const relLaneO = 0.5 + 0.4 * ez(frame, s.prChipAt, s.prChipAt + 10, 0, 1) // lane brightens at PR open

  // Rows (labels/tags/dots) share entrance timing keyed off their draw/fork frame.
  const mainRise = riseIn(frame, s.drawMain + 2, 8, 6)
  const mainCheckO = ez(frame, s.drawMain + 10, s.drawMain + 18, 0, 1)
  const relRise = riseIn(frame, s.drawRel + 12, 8, 6)
  const relTagO = ez(frame, s.drawRel + 18, s.drawRel + 26, 0, 1)
  const relDotPop = pop(frame, s.drawRel + 10)
  const relDotColor = interpolateColors(frame, [s.prChipAt, s.prChipAt + 6], [DOT_WORK, C.green])

  // PR chip.
  const chipPop = pop(frame, s.prChipAt)
  const chipO = ez(frame, s.prChipAt, s.prChipAt + 4, 0, 1)
  const prMerged = s.prMergedAt !== undefined && frame >= s.prMergedAt
  const prColor = prMerged ? C.indigoGlow : C.green

  return (
    <div style={{ position: "absolute", inset: 0, overflow: "hidden" }}>
      <div style={{ position: "absolute", left: 0, top: 0, width, height }}>
        <svg width={width} height={height} style={{ position: "absolute", left: 0, top: 0 }}>
          {/* main trunk lane */}
          <Draw d={`M ${LANE_X0} ${mainY} L ${MAIN_END} ${mainY}`} len={mainLen} drawn={mainDrawn} stroke={C.muted} o={0.28} />
          {/* main → rel connector + rel lane */}
          <Draw d={connD} len={connLen} drawn={connDrawn} stroke={C.muted} o={0.35} w={1} />
          <Draw d={`M ${LANE_X0} ${relY} L ${REL_END} ${relY}`} len={relLen} drawn={relDrawn} stroke={C.text} o={relLaneO} w={2} />
          {/* shared depth-2 rail (extends as lanes fork) */}
          {railBottom > relY + 9.5 ? (
            <line x1={railX} y1={relY + 9} x2={railX} y2={railBottom} stroke={C.muted} strokeOpacity={0.35} strokeWidth={1} />
          ) : null}
          {/* EXP lanes: tree stub → working lane → green merge sweep */}
          {exps.map((e) => {
            const stubLen = expDotX - 9 - railX
            const stubDrawn = ez(frame, e.forkAt + 6, e.forkAt + 16, 0, stubLen)
            const workEnd = e.mx - e.dy - WORK_GAP
            const wLen = workEnd - LANE_X0
            const wDrawn = ez(frame, e.forkAt + 12, e.forkAt + 28, 0, wLen)
            const r = e.dy / 2
            const mLen = WORK_GAP + Math.PI * r
            const mD = `M ${workEnd} ${e.y} L ${e.mx - e.dy} ${e.y} A ${r} ${r} 0 0 0 ${e.mx - r} ${e.y - r} A ${r} ${r} 0 0 1 ${e.mx} ${relY}`
            const mDrawn = ez(frame, e.mergeAt - 10, e.mergeAt, 0, mLen)
            return (
              <React.Fragment key={e.name}>
                <Draw d={`M ${railX} ${e.y} L ${expDotX - 9} ${e.y}`} len={stubLen} drawn={stubDrawn} stroke={C.muted} o={0.35} w={1} />
                <Draw d={`M ${LANE_X0} ${e.y} L ${workEnd} ${e.y}`} len={wLen} drawn={wDrawn} stroke={C.muted} o={0.35} />
                <Draw d={mD} len={mLen} drawn={mDrawn} stroke={C.green} o={0.75} />
              </React.Fragment>
            )
          })}
          {/* merge junctions: commit dot POP + 12px green pulse ring (8f) */}
          {exps.map((e) => {
            const jp = pop(frame, e.mergeAt)
            return (
              <React.Fragment key={`j-${e.name}`}>
                {jp > 0.01 ? <circle cx={e.mx} cy={relY} r={3.5 * Math.min(jp, 1.15)} fill={C.green} /> : null}
                {frame >= e.mergeAt && frame < e.mergeAt + 9 ? (
                  <circle
                    cx={e.mx}
                    cy={relY}
                    r={interpolate(frame, [e.mergeAt, e.mergeAt + 8], [4, 12], CLAMP)}
                    stroke={C.green}
                    strokeWidth={2}
                    fill="none"
                    opacity={interpolate(frame, [e.mergeAt, e.mergeAt + 8], [0.85, 0], CLAMP)}
                  />
                ) : null}
              </React.Fragment>
            )
          })}
        </svg>

        {/* main row — default branch: medium label, no state dot, trailing ✓ (ref: "master ✓") */}
        <div style={{ position: "absolute", left: padLeft, top: mainY - ROW_H / 2, height: ROW_H, display: "flex", alignItems: "center", gap: 10, ...mainRise }}>
          <span style={{ fontFamily: MONO_FONT, fontSize: 16, fontWeight: 700, color: C.text }}>main</span>
          <span style={{ display: "flex", color: C.text, opacity: mainCheckO * 0.85 }}>
            <CheckIcon />
          </span>
        </div>

        {/* rel row — exp/rel-v0-12 (8px PR-state dot, "worktree" tag) */}
        <span
          style={{
            position: "absolute",
            left: relDotX - 4,
            top: relY - 4,
            width: 8,
            height: 8,
            borderRadius: `50%`,
            backgroundColor: relDotColor,
            scale: String(relDotPop),
          }}
        />
        <div style={{ position: "absolute", left: relLabelX, top: relY - ROW_H / 2, height: ROW_H, display: "flex", alignItems: "center", ...relRise }}>
          <span style={{ fontFamily: MONO_FONT, fontSize: 16, color: C.text }}>{LANES[1]?.name ?? RELEASE.integrationBranch}</span>
          <span style={{ fontFamily: UI_FONT, fontSize: 12, color: C.dim, marginLeft: 14, opacity: relTagO }}>worktree</span>
        </div>

        {/* EXP rows */}
        {exps.map((e) => {
          const dotPop = pop(frame, e.forkAt + 12)
          const dotColor = interpolateColors(frame, [e.mergeAt - 8, e.mergeAt - 4], [DOT_WORK, C.green])
          const rise = riseIn(frame, e.forkAt + 12, 8, 6)
          const tagO = ez(frame, e.forkAt + 18, e.forkAt + 26, 0, 1)
          const upO = ez(frame, e.forkAt + 24, e.forkAt + 30, 0, 1) * (1 - ez(frame, e.mergeAt - 8, e.mergeAt - 2, 0, 1))
          return (
            <React.Fragment key={`row-${e.name}`}>
              <span
                style={{
                  position: "absolute",
                  left: expDotX - 4,
                  top: e.y - 4,
                  width: 8,
                  height: 8,
                  borderRadius: `50%`,
                  backgroundColor: dotColor,
                  scale: String(dotPop),
                }}
              />
              <div style={{ position: "absolute", left: expLabelX, top: e.y - ROW_H / 2, height: ROW_H, display: "flex", alignItems: "center", ...rise }}>
                <span style={{ fontFamily: MONO_FONT, fontSize: 16, color: C.text }}>{e.name}</span>
                {e.worktree ? (
                  <span style={{ fontFamily: UI_FONT, fontSize: 12, color: C.dim, marginLeft: 14, opacity: tagO }}>worktree</span>
                ) : null}
                <span style={{ fontFamily: MONO_FONT, fontSize: 12, color: C.dim, marginLeft: 12, opacity: upO }}>{e.up}</span>
              </div>
            </React.Fragment>
          )
        })}

        {/* PR chip — "⑂ PR #219 · open" pops onto the rel lane (POP) */}
        {frame >= s.prChipAt - 1 ? (
          <div
            style={{
              position: "absolute",
              left: REL_END + 14,
              top: relY - 13,
              height: 26,
              display: "flex",
              alignItems: "center",
              gap: 7,
              padding: `0 12px`,
              borderRadius: 13,
              backgroundColor: C.panel,
              border: `1px solid ${alpha(prColor, 0.45)}`,
              boxShadow: `0 6px 20px rgba(0,0,0,0.45)`,
              opacity: chipO,
              scale: String(chipPop),
              transformOrigin: `0 50%`,
            }}
          >
            <span style={{ display: "flex", color: prColor }}>
              <PrIcon />
            </span>
            <span style={{ fontFamily: MONO_FONT, fontSize: 13, color: C.text }}>{`PR #${RELEASE.pr}`}</span>
            <span style={{ fontFamily: MONO_FONT, fontSize: 13, color: C.dim }}>·</span>
            <span style={{ fontFamily: MONO_FONT, fontSize: 13, color: prColor }}>{prMerged ? `merged` : `open`}</span>
          </div>
        ) : null}
      </div>
    </div>
  )
}
