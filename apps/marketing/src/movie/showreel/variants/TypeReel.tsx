// variants/TypeReel.tsx — variation "Type" (EXP-1100 r2): kinetic
// typography cut on a beat. Five statements land huge, each punched out by
// a five-frame inverted flash; behind each word a low-opacity world of the
// thing it names (the board, the automations that run it, a scrolling tool
// stream, a PR chain that collapses into a check, the pipeline rolling
// feedback → release). A brand card ends it. No captions, no chrome: the
// type IS the picture. r3: aimed at the autopilot story.

import React from "react"
import { AbsoluteFill, random } from "remotion"
import { C, DISPLAY, MONO, UI, VIOLET } from "../theme"
import { IN, POP, SETTLE, SNAP, enter, lerp, mix, pop, seg, stagger } from "../motion"
import { Glass, Ident, IssueRow, Kinetic, Pill, Ripple, StatusGlyph, GroupBand } from "../atoms"
import { BrandCard, Decode, DigitRoll, timecode } from "../atoms-extra"
import { ClaudeMark, CodexMark } from "../../closedloop/surfaces/agentmarks"

const FLASH = 5

// Section windows: [at, out]; the flash occupies [out, out + FLASH].
const S = {
  header: { at: 0, out: 30 },
  app: { at: 30, out: 80 },
  autopilot: { at: 85, out: 142 },
  agents: { at: 147, out: 214 },
  ships: { at: 219, out: 300 },
  end: { at: 305, out: 384 },
  brand: { at: 388, out: 450 },
} as const

const FLASHES = [S.app.out, S.autopilot.out, S.agents.out, S.ships.out]

const inWin = (f: number, w: { at: number; out: number }) => f >= w.at && f < w.out + FLASH

// A statement: letters land, the whole word overshoots down to rest, then it
// is cut by the flash (no exit of its own).
const Word: React.FC<{
  f: number
  at: number
  text: string
  size?: number
  color?: string
  y?: number
  step?: number
}> = ({ f, at, text, size = 210, color = C.text, y = 540, step = 2 }) => {
  const s = 1.14 - 0.14 * pop(f, at, SETTLE)
  return (
    <div
      style={{
        position: `absolute`,
        left: 0,
        right: 0,
        top: y,
        translate: `0px -50%`,
        textAlign: `center`,
        fontFamily: DISPLAY,
        fontSize: size,
        fontWeight: 600,
        letterSpacing: `-0.05em`,
        lineHeight: 1,
        color,
        scale: String(s),
      }}
    >
      <Kinetic text={text} f={f} at={at} step={step} dur={16} rise={70} blur={18} />
    </div>
  )
}

// ── backgrounds ─────────────────────────────────────────────────────────────
const BoardWorld: React.FC<{ l: number }> = ({ l }) => (
  <div
    style={{
      position: `absolute`,
      left: 160,
      top: 90,
      width: 1600,
      opacity: 0.32 * seg(l, 0, 14),
      scale: String(mix(l, 0, 62, 1.02, 1.08)),
      translate: `0px ${mix(l, 0, 62, 20, -30)}px`,
      filter: `blur(${mix(l, 0, 20, 8, 1.5)}px)`,
    }}
  >
    <Glass x={0} y={0} w={1600} h={900} r={20}>
      <GroupBand label="In progress" count={3} status="progress" h={54} font={20} />
      {[
        [`EXP-1100`, `Ship the 15-second showreel`, `progress`],
        [`EXP-1097`, `Review wave clears its layer before landing`, `progress`],
        [`EXP-1093`, `Issue rail hover opens the mini-graph`, `review`],
      ].map(([id, t, st], i) => (
        <div key={id} style={enter(l, stagger(4, i, 4), 12, { rise: 0, x: -40, blur: 0 })}>
          <IssueRow ident={id} title={t} status={st as `progress`} prio={(3 - i) as 1 | 2 | 3} h={76} font={28} glyph={26} assignee={{ initials: [`DS`, `MK`, `AL`][i], hue: [6, 4, 1][i] }} />
        </div>
      ))}
      <GroupBand label="Backlog" count={5} status="backlog" h={54} font={20} />
      {[
        [`EXP-1102`, `Workflow host state lives on the device`],
        [`EXP-1104`, `Android: unbroken tee on tree connectors`],
        [`EXP-1105`, `Digest picks the reader's local hour`],
        [`EXP-1106`, `Passkey login through the desktop handoff`],
        [`EXP-1108`, `Blueprint reel: packets ride the connectors`],
      ].map(([id, t], i) => (
        <div key={id} style={enter(l, stagger(16, i, 4), 12, { rise: 0, x: -40, blur: 0 })}>
          <IssueRow ident={id} title={t} status="backlog" prio={1} h={76} font={28} glyph={26} />
        </div>
      ))}
    </Glass>
  </div>
)

const TOOL_LINES = [
  `● Read  src/lib/issue-graph.ts`,
  `● Grep  resolveTeamAccess · 9 matches`,
  `● Edit  components/issue-rail.tsx  +48 −12`,
  `● Bash  bun run typecheck`,
  `  ✓ 0 errors`,
  `● Write lib/pr-stack.test.ts`,
  `● Bash  bun run test -- pr-stack`,
  `  ✓ 14 passed`,
  `● Edit  crates/coding/src/skill.md  +6 −2`,
  `● Bash  git push -u origin exp/EXP-1100`,
  `● exponential_pr_open  EXP-1100`,
  `✳ Rendering 450 frames…`,
]

const StreamWorld: React.FC<{ l: number }> = ({ l }) => (
  <div style={{ position: `absolute`, inset: 0, opacity: 0.38 * seg(l, 0, 12), fontFamily: MONO, fontSize: 26, lineHeight: `52px`, color: C.text }}>
    {[0, 1, 2].map((col) => {
      const speed = [3.2, 4.6, 2.6][col]
      const off = (l * speed) % (TOOL_LINES.length * 52)
      return (
        <div key={col} style={{ position: `absolute`, left: 80 + col * 620, top: 0, width: 600, height: 1080, overflow: `hidden`, WebkitMaskImage: `linear-gradient(to bottom, transparent, black 15%, black 85%, transparent)`, maskImage: `linear-gradient(to bottom, transparent, black 15%, black 85%, transparent)` }}>
          <div style={{ translate: `0px ${-off + 200 * col}px`, whiteSpace: `pre` }}>
            {[...TOOL_LINES, ...TOOL_LINES, ...TOOL_LINES].map((t, i) => (
              <div key={i} style={{ color: t.startsWith(`  ✓`) ? C.green : t.startsWith(`✳`) ? C.termSpinner : C.text }}>
                {t}
              </div>
            ))}
          </div>
        </div>
      )
    })}
  </div>
)

const AUTOMATIONS: [string, string, string][] = [
  [`⏱`, `nightly · 02:00`, `Triage the feedback inbox`],
  [`⚡`, `on feedback`, `File the issue · start a run`],
  [`⚡`, `on PR opened`, `Review wave · auto-merge`],
  [`⏱`, `weekly · Mon`, `Plan the workflow from the backlog`],
]

const AutoWorld: React.FC<{ l: number }> = ({ l }) => (
  <div style={{ position: `absolute`, left: 260, top: 150, width: 1400, opacity: 0.34 * seg(l, 0, 14), filter: `blur(${mix(l, 0, 20, 8, 1.5)}px)`, scale: String(mix(l, 0, 57, 1.0, 1.06)) }}>
    <Glass x={0} y={0} w={1400} h={780} r={20}>
      <div style={{ display: `flex`, alignItems: `center`, height: 64, padding: `0 28px`, borderBottom: `1px solid ${C.strokeSection}`, fontFamily: UI, fontSize: 24, fontWeight: 600, color: C.text }}>
        Automations <span style={{ color: C.dim, fontWeight: 500, marginLeft: 12 }}>/ macbook-pro</span>
      </div>
      {AUTOMATIONS.map(([glyph, when, what], i) => (
        <div key={when} style={{ display: `flex`, alignItems: `center`, gap: 22, height: 120, padding: `0 28px`, borderBottom: `1px solid ${C.strokeRow}`, fontFamily: UI, fontSize: 30, color: C.text, ...enter(l, stagger(6, i, 5), 12, { rise: 0, x: -40, blur: 0 }) }}>
          <span style={{ width: 44, textAlign: `center`, color: C.green, fontSize: 28 }}>{glyph}</span>
          <span style={{ fontFamily: MONO, fontSize: 22, color: C.muted, width: 260 }}>{when}</span>
          <span style={{ fontWeight: 500, flex: 1 }}>{what}</span>
          <span style={{ width: 64, height: 32, borderRadius: 16, background: C.green, position: `relative` }}>
            <span style={{ position: `absolute`, right: 4, top: 4, width: 24, height: 24, borderRadius: `50%`, background: C.text }} />
          </span>
        </div>
      ))}
    </Glass>
  </div>
)

const STAGES = [`Feedback.`, `Issue.`, `Run.`, `Pull request.`, `Merge.`, `Release.`]
const ROLL_EVERY = 12

const StageRoll: React.FC<{ l: number }> = ({ l }) => {
  const idx = Math.min(STAGES.length - 1, Math.floor(l / ROLL_EVERY))
  const p = seg(l - idx * ROLL_EVERY, 0, 5)
  const prev = idx > 0 ? STAGES[idx - 1] : null
  const cur = STAGES[idx]
  const line = (text: string, y: number, o: number, b: number) => (
    <div
      key={text}
      style={{
        position: `absolute`,
        left: 0,
        right: 0,
        top: 0,
        textAlign: `center`,
        translate: `0px ${y}px`,
        opacity: o,
        filter: b > 0.2 ? `blur(${b}px)` : undefined,
      }}
    >
      {text}
    </div>
  )
  return (
    <div style={{ position: `absolute`, left: 0, right: 0, top: 560, height: 260, overflow: `hidden`, fontFamily: DISPLAY, fontSize: 190, fontWeight: 600, letterSpacing: `-0.05em`, lineHeight: 1.2, color: C.text }}>
      {prev ? line(prev, -240 * p, 1 - p, 12 * p) : null}
      {line(cur, 240 * (1 - p), p, 12 * (1 - p))}
    </div>
  )
}

const CHAIN = [`EXP-1097`, `EXP-1098`, `EXP-1100`]
const PARTS = Array.from({ length: 48 }, (_, i) => ({
  a: random(`ta${i}`) * Math.PI * 2,
  v: 8 + random(`tv${i}`) * 16,
  s: 6 + random(`ts${i}`) * 8,
  c: [C.statusDone, C.statusInReview, VIOLET, C.text][i % 4],
}))

const ShippedWorld: React.FC<{ l: number }> = ({ l }) => {
  // chips in [0, 30), collapse [30, 44), check pops at 42, word at 50.
  const collapse = seg(l, 30, 44, IN)
  const check = pop(l, 42, SNAP)
  const burst = l - 44
  return (
    <>
      {CHAIN.map((id, i) => {
        const x0 = 960 + (i - 1) * 420
        const x = lerp(x0, 960, collapse)
        const s = pop(l, stagger(2, i, 6), POP)
        return (
          <div key={id} style={{ position: `absolute`, left: x - 150, top: 330, width: 300, height: 70, borderRadius: 16, background: `rgba(20,20,24,0.98)`, border: `1px solid ${C.strokeActive}`, boxShadow: `0 0 0 1px rgba(34,197,94,0.35), 0 20px 60px -20px rgba(0,0,0,0.8)`, display: `flex`, alignItems: `center`, justifyContent: `center`, gap: 14, opacity: Math.min(1, s * 1.4) * (1 - collapse), scale: String((0.6 + 0.4 * s) * (1 - 0.4 * collapse)) }}>
            <StatusGlyph kind="review" size={24} />
            <Ident text={id} size={24} color={C.text} />
          </div>
        )
      })}
      {[0, 1].map((i) => {
        const draw = seg(l, 12 + i * 5, 24 + i * 5)
        const x0 = 960 + (i - 1) * 420 + 150
        return (
          <div key={i} style={{ position: `absolute`, left: x0, top: 364, height: 2, width: 120 * draw, background: C.statusInReview, opacity: 1 - collapse }} />
        )
      })}
      <div style={{ position: `absolute`, left: 960 - 80, top: 365 - 80, width: 160, height: 160, scale: String(check * (1 + 0.25 * seg(l, 60, 84))), opacity: check > 0.01 ? 1 : 0, filter: `drop-shadow(0 0 60px rgba(59,130,246,0.7))` }}>
        <StatusGlyph kind="done" size={160} />
      </div>
      <Ripple f={l} at={42} x={960} y={365} size={420} color={C.statusDone} width={3} dur={28} />
      {burst >= 0 && burst < 36
        ? PARTS.map((p, i) => {
            const t = burst
            const x = 960 + Math.cos(p.a) * p.v * t * (1 - t / 90)
            const y = 365 + Math.sin(p.a) * p.v * t * (1 - t / 90) + 0.3 * t * t
            return <div key={i} style={{ position: `absolute`, left: x - p.s / 2, top: y - p.s / 2, width: p.s, height: p.s, borderRadius: i % 3 === 0 ? `50%` : 2, background: p.c, opacity: Math.max(0, 1 - t / 32) }} />
          })
        : null}
      <Word f={l} at={50} text="Ships itself." size={210} y={640} color={C.text} />
    </>
  )
}

// ── the reel ────────────────────────────────────────────────────────────────
export const TypeReel: React.FC<{ f: number }> = ({ f }) => {
  const flashAt = FLASHES.find((o) => f >= o && f < o + FLASH)
  const flashT = flashAt === undefined ? 0 : 1 - (f - flashAt) / FLASH
  const inverted = flashAt !== undefined
  const ink = inverted ? `#0a0a0a` : C.text

  // Section index for the counter.
  const sections = [S.app, S.autopilot, S.agents, S.ships, S.end]
  const idx = sections.reduce((acc, w, i) => (f >= w.at ? i : acc), -1)

  return (
    <AbsoluteFill style={{ backgroundColor: inverted ? `#f4f4f5` : C.canvas, overflow: `hidden`, fontFamily: UI, color: C.text }}>
      {/* grid + vignette */}
      {!inverted ? (
        <>
          <AbsoluteFill style={{ backgroundImage: `linear-gradient(rgba(255,255,255,0.045) 1px, transparent 1px), linear-gradient(90deg, rgba(255,255,255,0.045) 1px, transparent 1px)`, backgroundSize: `120px 120px`, opacity: seg(f, 0, 30) }} />
          <AbsoluteFill style={{ background: `radial-gradient(ellipse 80% 70% at 50% 50%, transparent 40%, rgba(0,0,0,0.7))` }} />
        </>
      ) : null}

      {/* header + timecode + counter */}
      <div style={{ position: `absolute`, left: 72, top: 52, fontFamily: MONO, fontSize: 18, letterSpacing: `0.22em`, color: inverted ? `#0a0a0a` : C.muted }}>
        <Decode text="EXPONENTIAL / SHOWREEL 2026" f={f} at={4} dur={40} />
      </div>
      <div style={{ position: `absolute`, right: 72, top: 52, fontFamily: MONO, fontSize: 18, letterSpacing: `0.12em`, color: inverted ? `#0a0a0a` : C.muted, opacity: seg(f, 20, 34) }}>
        TC {timecode(f)}
      </div>
      {idx >= 0 && f < S.brand.at ? (
        <div style={{ position: `absolute`, right: 72, bottom: 52, fontFamily: MONO, fontSize: 18, letterSpacing: `0.22em`, color: inverted ? `#0a0a0a` : C.muted, display: `flex`, lineHeight: 1, alignItems: `flex-start` }}>
          <span style={{ lineHeight: 1 }}>0</span>
          <DigitRoll f={f} at={sections[idx].at} dur={8} from={Math.max(0, idx)} to={idx + 1} size={18} />
          <span style={{ lineHeight: 1 }}>&nbsp;/ 05</span>
        </div>
      ) : null}

      {/* 01 — your app */}
      {inWin(f, S.app) ? (
        <>
          {!inverted ? <BoardWorld l={f - S.app.at} /> : null}
          <Word f={f} at={S.app.at + 2} text="Your app" color={ink} />
        </>
      ) : null}

      {/* 02 — on autopilot */}
      {inWin(f, S.autopilot) ? (
        <>
          {!inverted ? <AutoWorld l={f - S.autopilot.at} /> : null}
          <Word f={f} at={S.autopilot.at + 2} text="on autopilot." color={ink} />
        </>
      ) : null}

      {/* 03 — agents build it */}
      {inWin(f, S.agents) ? (
        <>
          {!inverted ? <StreamWorld l={f - S.agents.at} /> : null}
          <Word f={f} at={S.agents.at + 2} text="Agents build it." size={190} color={ink} y={500} />
          <div style={{ position: `absolute`, left: 0, right: 0, top: 660, display: `flex`, justifyContent: `center`, gap: 18, ...enter(f, S.agents.at + 28, 14, { rise: 20, blur: 8 }) }}>
            <Pill size={26} color={ink} fill={inverted ? `rgba(0,0,0,0.06)` : C.fillActive} stroke={inverted ? `rgba(0,0,0,0.2)` : C.strokeActive}>
              <ClaudeMark size={26} /> claude
            </Pill>
            <Pill size={26} color={ink} fill={inverted ? `rgba(0,0,0,0.06)` : C.fillActive} stroke={inverted ? `rgba(0,0,0,0.2)` : C.strokeActive}>
              <CodexMark size={26} /> codex
            </Pill>
            <Pill size={26} color={ink} fill={inverted ? `rgba(0,0,0,0.06)` : C.fillActive} stroke={inverted ? `rgba(0,0,0,0.2)` : C.strokeActive}>
              on your hardware · unattended
            </Pill>
          </div>
        </>
      ) : null}

      {/* 04 — ships itself */}
      {inWin(f, S.ships) ? <ShippedWorld l={f - S.ships.at} /> : null}

      {/* 05 — end to end */}
      {inWin(f, S.end) ? (
        <>
          <Word f={f} at={S.end.at + 2} text="End to end." size={180} y={430} color={ink} />
          <div style={{ color: ink, opacity: seg(f, S.end.at + 8, S.end.at + 16) }}>
            <StageRoll l={f - S.end.at - 6} />
          </div>
        </>
      ) : null}

      {/* brand */}
      {f >= S.brand.at ? <BrandCard f={f} at={S.brand.at} sub="exponential.at · automate software end to end" /> : null}

      {/* the flash decay */}
      {inverted ? <AbsoluteFill style={{ backgroundColor: `#ffffff`, opacity: flashT * 0.5, pointerEvents: `none` }} /> : null}

      {/* progress hairline */}
      <div style={{ position: `absolute`, left: 0, bottom: 0, height: 3, width: (f / 450) * 1920, background: inverted ? `#0a0a0a` : `linear-gradient(to right, ${VIOLET}, ${C.green})`, opacity: 0.8 }} />
    </AbsoluteFill>
  )
}
