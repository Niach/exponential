// acts/Agents.tsx — the run (5.1–8.3 s): the activity stream on the left
// (tool calls landing one after the other under the Claude mark, a spinner
// for the turn), the diff growing on the right with a rolling +/− counter,
// and a steer message typed and queued mid-turn. Panels part with parallax;
// the act leaves through a slanted wipe that reveals the four clients.

import React from "react"
import { AbsoluteFill } from "remotion"
import { C, MONO, UI } from "../theme"
import { IN, POP, SNAP, enter, mix, pop, roll, stagger, typed } from "../motion"
import { Caption, Glass, Ident, LiveDot, Pill } from "../atoms"
import { ClaudeMark } from "../../closedloop/surfaces/agentmarks"

const LX = 190
const LY = 118
const LW = 800
const LH = 612
const RX = 1020
const RY = 118
const RW = 710
const RH = 612

type Line =
  | { kind: `tool`; name: string; arg: string }
  | { kind: `prose`; text: string }
  | { kind: `ok`; text: string }
  | { kind: `spin`; text: string }

const STREAM: Line[] = [
  { kind: `tool`, name: `Read`, arg: `src/movie/ships/theme.ts` },
  { kind: `tool`, name: `Grep`, arg: `useCurrentFrame · 14 matches` },
  { kind: `prose`, text: `Building the reel on the marketing Remotion rig.` },
  { kind: `tool`, name: `Write`, arg: `showreel/acts/Ship.tsx` },
  { kind: `tool`, name: `Bash`, arg: `bun run typecheck` },
  { kind: `ok`, text: `0 errors` },
  { kind: `tool`, name: `Edit`, arg: `showreel/acts/Ship.tsx  +96 −4` },
  { kind: `spin`, text: `Rendering 450 frames…` },
]
const STREAM_AT = 12
const STREAM_STEP = 7

type Diff = { t: `hunk` | `ctx` | `add` | `del`; s: string }
const DIFF: Diff[] = [
  { t: `hunk`, s: `@@ -41,9 +41,14 @@ export const Ship` },
  { t: `ctx`, s: `  const wave = seg(l, 52, 74)` },
  { t: `del`, s: `  const sweep = lerp(0, W, wave)` },
  { t: `add`, s: `  const sweep = lerp(-200, W + 200, wave)` },
  { t: `add`, s: `  const front = (x: number) => seg(sweep, x - 80, x + 40)` },
  { t: `ctx`, s: `` },
  { t: `ctx`, s: `  return nodes.map((n) => {` },
  { t: `del`, s: `    const done = wave > 0.5` },
  { t: `add`, s: `    const done = front(n.x)` },
  { t: `add`, s: `    const ring = pop(l, n.at + 2, SNAP)` },
  { t: `ctx`, s: `    return (` },
  { t: `del`, s: `      <Node key={n.id} {...n} done={done} />` },
  { t: `add`, s: `      <Node key={n.id} {...n} done={done} ring={ring} />` },
]
const DIFF_AT = 18
const DIFF_STEP = 3

const STEER = `make the merge sweep snappier`
const STEER_AT = 50
const SEND_AT = 78

const DIFF_COLOR: Record<Diff[`t`], { bg: string; fg: string; sign: string }> = {
  hunk: { bg: C.hunkBg, fg: C.hunkFg, sign: `` },
  ctx: { bg: `transparent`, fg: `rgba(250,250,250,0.72)`, sign: ` ` },
  add: { bg: C.diffAddBg, fg: C.text, sign: `+` },
  del: { bg: C.diffDelBg, fg: C.text, sign: `−` },
}

export const Agents: React.FC<{ l: number }> = ({ l }) => {
  // Parallax push-in; the wipe carries the exit.
  const push = mix(l, 0, 92, 1, 1.045)
  const parallax = mix(l, 0, 92, 0, 14)
  const wipe = mix(l, 84, 96, -12, 112, IN)
  const clip = `polygon(${wipe}% 0, 120% 0, 120% 100%, ${wipe - 10}% 100%)`
  // Everything left of the slanted edge is gone: the clients show through.

  const spinAngle = (l * 14) % 360
  const added = roll(l, 30, 66, 0, 96)
  const removed = roll(l, 34, 60, 0, 4)
  const steerTyped = typed(STEER, l, STEER_AT, 1.2)
  const sendPop = pop(l, SEND_AT, SNAP)
  const queuedPop = pop(l, SEND_AT + 3, POP)

  return (
    <AbsoluteFill style={{ clipPath: clip }}>
      <div style={{ position: `absolute`, inset: 0, scale: String(push) }}>
        {/* ── activity stream ── */}
        <div style={{ position: `absolute`, inset: 0, translate: `${-parallax}px 0px` }}>
        <div style={{ position: `absolute`, inset: 0, ...enter(l, 0, 16, { rise: 0, x: -60, blur: 12 }) }}>
          <Glass x={LX} y={LY} w={LW} h={LH} r={18}>
            <div
              style={{
                display: `flex`,
                alignItems: `center`,
                gap: 12,
                height: 60,
                padding: `0 24px`,
                borderBottom: `1px solid ${C.strokeSection}`,
                fontFamily: UI,
                fontSize: 18,
                color: C.text,
              }}
            >
              <ClaudeMark size={22} />
              <span style={{ fontWeight: 600 }}>claude</span>
              <span style={{ fontFamily: MONO, color: C.dim, fontSize: 15 }}>opus · high</span>
              <span style={{ flex: 1 }} />
              <LiveDot f={l} size={9} />
              <span style={{ fontFamily: MONO, fontSize: 14, color: C.muted, letterSpacing: `0.08em` }}>RUNNING</span>
              <Ident text="EXP-1100" size={15} color={C.muted} />
            </div>

            <div style={{ padding: `18px 24px 0`, fontFamily: MONO, fontSize: 20, lineHeight: 1.35 }}>
              {STREAM.map((line, i) => {
                const at = stagger(STREAM_AT, i, STREAM_STEP)
                const style = enter(l, at, 12, { rise: 12, x: -8, blur: 4 })
                const isLast = i === STREAM.length - 1
                return (
                  <div key={i} style={{ display: `flex`, alignItems: `baseline`, gap: 12, minHeight: 36, ...style }}>
                    {line.kind === `tool` ? (
                      <>
                        <span style={{ color: C.termToolDot }}>●</span>
                        <span style={{ color: C.text, fontWeight: 600 }}>{line.name}</span>
                        <span style={{ color: C.muted, overflow: `hidden`, whiteSpace: `nowrap`, textOverflow: `ellipsis` }}>{line.arg}</span>
                      </>
                    ) : line.kind === `prose` ? (
                      <>
                        <span style={{ color: C.termProseDot }}>●</span>
                        <span style={{ color: C.text, fontFamily: UI, fontSize: 20 }}>{line.text}</span>
                      </>
                    ) : line.kind === `ok` ? (
                      <>
                        <span style={{ color: C.green, marginLeft: 26 }}>✓</span>
                        <span style={{ color: C.green }}>{line.text}</span>
                      </>
                    ) : (
                      <>
                        <span
                          style={{
                            display: `inline-block`,
                            color: C.termSpinner,
                            rotate: `${spinAngle}deg`,
                            transformOrigin: `50% 55%`,
                          }}
                        >
                          ✳
                        </span>
                        <span style={{ color: C.termSpinner }}>{line.text}</span>
                        {isLast ? <span style={{ color: C.dim, fontSize: 15 }}>esc to interrupt</span> : null}
                      </>
                    )}
                  </div>
                )
              })}
            </div>

            {/* the composer — a steer typed mid-turn */}
            <div
              style={{
                position: `absolute`,
                left: 24,
                right: 24,
                bottom: 24,
                ...enter(l, 40, 14, { rise: 16, blur: 6 }),
              }}
            >
              <div
                style={{
                  display: `flex`,
                  alignItems: `center`,
                  gap: 12,
                  height: 64,
                  padding: `0 12px 0 20px`,
                  borderRadius: 14,
                  background: C.fillRow,
                  border: `1px solid ${l >= STEER_AT && l < SEND_AT ? C.strokeActive : C.strokeCard}`,
                  fontFamily: UI,
                  fontSize: 20,
                  color: C.text,
                }}
              >
                <span style={{ flex: 1, color: steerTyped ? C.text : C.dim }}>
                  {steerTyped || `Steer the run…`}
                  {l >= STEER_AT - 6 && l < SEND_AT && l % 14 < 8 ? (
                    <span style={{ display: `inline-block`, width: 2, height: `1em`, background: C.text, verticalAlign: `-0.15em`, marginLeft: 1 }} />
                  ) : null}
                </span>
                <span style={{ scale: String(queuedPop), opacity: queuedPop > 0.01 ? 1 : 0 }}>
                  <Pill size={14} color={C.statusInProgress} fill="rgba(234,179,8,0.12)" stroke="rgba(234,179,8,0.35)">
                    1 queued
                  </Pill>
                </span>
                <span
                  style={{
                    display: `inline-flex`,
                    alignItems: `center`,
                    justifyContent: `center`,
                    width: 40,
                    height: 40,
                    borderRadius: 10,
                    background: C.primary,
                    color: C.primaryFg,
                    scale: String(1 - 0.16 * Math.sin(Math.min(1, Math.max(0, sendPop)) * Math.PI)),
                  }}
                >
                  <svg width={20} height={20} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round">
                    <path d="M12 19V5" />
                    <path d="m5 12 7-7 7 7" />
                  </svg>
                </span>
              </div>
            </div>
          </Glass>
        </div>
        </div>

        {/* ── diff ── */}
        <div style={{ position: `absolute`, inset: 0, translate: `${parallax}px 0px` }}>
        <div style={{ position: `absolute`, inset: 0, ...enter(l, 4, 16, { rise: 0, x: 60, blur: 12 }) }}>
          <Glass x={RX} y={RY} w={RW} h={RH} r={18}>
            <div
              style={{
                display: `flex`,
                alignItems: `center`,
                gap: 12,
                height: 60,
                padding: `0 22px`,
                borderBottom: `1px solid ${C.strokeSection}`,
                fontFamily: MONO,
                fontSize: 17,
                color: C.text,
              }}
            >
              <span style={{ color: C.muted }}>showreel/acts/</span>
              <span style={{ fontWeight: 600, marginLeft: -12 }}>Ship.tsx</span>
              <span style={{ flex: 1 }} />
              <span style={{ color: C.diffAdd }}>+{added}</span>
              <span style={{ color: C.diffDel }}>−{removed}</span>
            </div>
            <div style={{ padding: `14px 0`, fontFamily: MONO, fontSize: 17, lineHeight: `29px` }}>
              {DIFF.map((d, i) => {
                const c = DIFF_COLOR[d.t]
                const st = enter(l, stagger(DIFF_AT, i, DIFF_STEP), 10, { rise: 0, x: 40, blur: 3 })
                return (
                  <div key={i} style={{ display: `flex`, background: c.bg, color: c.fg, ...st }}>
                    <span style={{ width: 54, textAlign: `right`, paddingRight: 14, color: C.dim, fontSize: 14, flexShrink: 0 }}>
                      {d.t === `hunk` ? `` : 41 + i}
                    </span>
                    <span style={{ width: 22, color: d.t === `add` ? C.diffAdd : d.t === `del` ? C.diffDel : C.dim, flexShrink: 0 }}>{c.sign}</span>
                    <span style={{ whiteSpace: `pre`, overflow: `hidden`, textOverflow: `ellipsis`, fontStyle: d.t === `hunk` ? `italic` : undefined }}>{d.s}</span>
                  </div>
                )
              })}
            </div>
            {/* the rolling counter */}
            <div
              style={{
                position: `absolute`,
                right: 26,
                bottom: 20,
                display: `flex`,
                gap: 18,
                fontFamily: MONO,
                fontSize: 54,
                fontWeight: 600,
                letterSpacing: `-0.03em`,
                ...enter(l, 30, 14, { rise: 20, blur: 8 }),
              }}
            >
              <span style={{ color: C.diffAdd }}>+{added}</span>
              <span style={{ color: C.diffDel }}>−{removed}</span>
            </div>
          </Glass>
        </div>
        </div>
      </div>

      <Caption f={l} at={22} out={80} index="02 / Agents" line="Agents run on your hardware. Steer them live." />
    </AbsoluteFill>
  )
}
