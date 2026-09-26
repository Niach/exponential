// variants/BlueprintReel.tsx — variation "Blueprint" (EXP-1100 r2): the
// system as a technical drawing. Mono type only, one accent. Four device
// outlines draw themselves, the Postgres→Electric hub lands in the middle,
// connectors draw and packets start riding them; the agent and GitHub
// blocks join and one highlighted packet runs the full loop while the
// status ticker flips backlog → in progress → in review → done. Timecode,
// crosshairs, scanlines. The mark draws itself as the last stroke.

import React from "react"
import { AbsoluteFill } from "remotion"
import { C, MONO } from "../theme"
import { IN, SETTLE, enter, lerp, mix, pop, seg } from "../motion"
import { StatusGlyph, type StatusKind } from "../atoms"
import { BrandCard, Decode, timecode } from "../atoms-extra"

const INK = `rgba(250,250,250,0.9)`
const LINE = `rgba(255,255,255,0.32)`
const FAINT = `rgba(255,255,255,0.14)`
const G = C.green

type Pt = { x: number; y: number }
const HUB: Pt = { x: 960, y: 640 }
const AGENT: Pt = { x: 330, y: 640 }
const GITHUB: Pt = { x: 1590, y: 640 }

const DEVICES = [
  { id: `desktop`, x: 330, y: 250, label: `DESKTOP · RUST / GPUI`, at: 30 },
  { id: `web`, x: 760, y: 250, label: `WEB · TANSTACK START`, at: 40 },
  { id: `ios`, x: 1160, y: 250, label: `IOS · SWIFTUI`, at: 50 },
  { id: `android`, x: 1590, y: 250, label: `ANDROID · COMPOSE`, at: 60 },
] as const

// Quadratic arcs with analytic points (the packet needs positions).
const q = (a: Pt, c: Pt, b: Pt, t: number): Pt => ({
  x: (1 - t) ** 2 * a.x + 2 * (1 - t) * t * c.x + t ** 2 * b.x,
  y: (1 - t) ** 2 * a.y + 2 * (1 - t) * t * c.y + t ** 2 * b.y,
})
const qPath = (a: Pt, c: Pt, b: Pt) => `M ${a.x} ${a.y} Q ${c.x} ${c.y} ${b.x} ${b.y}`

const HUB_TO_AGENT = { a: { x: HUB.x - 82, y: HUB.y }, c: { x: 645, y: 560 }, b: { x: AGENT.x + 150, y: AGENT.y } }
const AGENT_TO_GITHUB = { a: { x: AGENT.x, y: AGENT.y + 70 }, c: { x: 960, y: 900 }, b: { x: GITHUB.x, y: GITHUB.y + 70 } }
const GITHUB_TO_HUB = { a: { x: GITHUB.x - 150, y: GITHUB.y }, c: { x: 1275, y: 560 }, b: { x: HUB.x + 82, y: HUB.y } }

const STATUS_STEPS: { kind: StatusKind; label: string }[] = [
  { kind: `backlog`, label: `BACKLOG` },
  { kind: `progress`, label: `IN PROGRESS` },
  { kind: `review`, label: `IN REVIEW` },
  { kind: `done`, label: `DONE` },
]

// Draw-on stroke props.
const draw = (t: number) => ({ pathLength: 1, strokeDasharray: 1, strokeDashoffset: 1 - t })

const Device: React.FC<{ d: (typeof DEVICES)[number]; f: number }> = ({ d, f }) => {
  const t = seg(f, d.at, d.at + 26)
  const stroke = { fill: `none`, stroke: INK, strokeWidth: 2, strokeLinecap: `round` as const, strokeLinejoin: `round` as const }
  const { x, y } = d
  return (
    <g>
      {d.id === `desktop` ? (
        <>
          <rect x={x - 150} y={y - 95} width={300} height={190} rx={10} {...stroke} {...draw(t)} />
          <line x1={x - 150} y1={y - 62} x2={x + 150} y2={y - 62} {...stroke} {...draw(seg(f, d.at + 10, d.at + 26))} />
          <line x1={x - 110} y1={y - 62} x2={x - 110} y2={y + 95} {...stroke} {...draw(seg(f, d.at + 14, d.at + 30))} />
          {[0, 1, 2].map((i) => (
            <circle key={i} cx={x - 132 + i * 16} cy={y - 78} r={4} {...stroke} {...draw(seg(f, d.at + 16 + i * 2, d.at + 24 + i * 2))} />
          ))}
        </>
      ) : d.id === `web` ? (
        <>
          <rect x={x - 130} y={y - 90} width={260} height={180} rx={10} {...stroke} {...draw(t)} />
          <line x1={x - 130} y1={y - 56} x2={x + 130} y2={y - 56} {...stroke} {...draw(seg(f, d.at + 10, d.at + 26))} />
          <rect x={x - 70} y={y - 80} width={190} height={14} rx={7} {...stroke} {...draw(seg(f, d.at + 14, d.at + 30))} />
        </>
      ) : (
        <>
          <rect x={x - 55} y={y - 110} width={110} height={220} rx={d.id === `ios` ? 22 : 16} {...stroke} {...draw(t)} />
          <rect x={x - 45} y={y - 100} width={90} height={200} rx={d.id === `ios` ? 14 : 10} {...stroke} strokeWidth={1} {...draw(seg(f, d.at + 8, d.at + 30))} opacity={0.5} />
          {d.id === `ios` ? <rect x={x - 22} y={y - 96} width={44} height={10} rx={5} {...stroke} {...draw(seg(f, d.at + 14, d.at + 26))} /> : <circle cx={x} cy={y - 90} r={4} {...stroke} {...draw(seg(f, d.at + 14, d.at + 26))} />}
          <line x1={x - 20} y1={y + 92} x2={x + 20} y2={y + 92} {...stroke} {...draw(seg(f, d.at + 16, d.at + 28))} />
        </>
      )}
      {/* the same three rows on every screen */}
      {[0, 1, 2].map((i) => {
        const w = d.id === `desktop` ? 210 : d.id === `web` ? 220 : 60
        const x0 = d.id === `desktop` ? x - 95 : d.id === `web` ? x - 110 : x - 30
        const y0 = (d.id === `desktop` ? y - 40 : d.id === `web` ? y - 34 : y - 60) + i * 26
        return <line key={i} x1={x0} y1={y0} x2={x0 + w * (i === 0 ? 1 : 0.7 - i * 0.15)} y2={y0} stroke={i === 0 ? G : FAINT} strokeWidth={i === 0 ? 3 : 2} {...draw(seg(f, d.at + 20 + i * 4, d.at + 32 + i * 4))} strokeLinecap="round" />
      })}
    </g>
  )
}

export const BlueprintReel: React.FC<{ f: number }> = ({ f }) => {
  const gridO = seg(f, 0, 30) * 0.55
  const hubT = seg(f, 118, 146)
  const shapes = Math.round(mix(f, 140, 184, 0, 25))
  const linksT = (i: number) => seg(f, 150 + i * 5, 176 + i * 5)
  const flow = f >= 180 ? -f * 2.4 : 0
  const agentT = seg(f, 228, 256)
  const ghT = seg(f, 238, 266)
  const loopT = (i: number) => seg(f, 250 + i * 8, 276 + i * 8)

  // The highlighted packet's loop: hub→agent 330–352, agent→github 352–378, github→hub 378–400.
  const p1 = seg(f, 330, 352, IN)
  const p2 = seg(f, 352, 378, IN)
  const p3 = seg(f, 378, 400, IN)
  const packet = f < 330 ? null : f < 352 ? q(HUB_TO_AGENT.a, HUB_TO_AGENT.c, HUB_TO_AGENT.b, p1) : f < 378 ? q(AGENT_TO_GITHUB.a, AGENT_TO_GITHUB.c, AGENT_TO_GITHUB.b, p2) : f < 400 ? q(GITHUB_TO_HUB.a, GITHUB_TO_HUB.c, GITHUB_TO_HUB.b, p3) : null
  const stepIdx = f >= 400 ? 3 : f >= 378 ? 2 : f >= 352 ? 1 : 0
  const stepPop = pop(f, [318, 352, 378, 400][stepIdx], SETTLE)

  const fade = seg(f, 404, 418, IN)
  const diagramO = 1 - fade * 0.85

  const cross = (x: number, y: number) => (
    <g stroke={LINE} strokeWidth={1.5}>
      <line x1={x - 14} y1={y} x2={x + 14} y2={y} />
      <line x1={x} y1={y - 14} x2={x} y2={y + 14} />
    </g>
  )

  return (
    <AbsoluteFill style={{ backgroundColor: `#050507`, overflow: `hidden`, fontFamily: MONO, color: INK }}>
      {/* grid */}
      <AbsoluteFill style={{ opacity: gridO, backgroundImage: `linear-gradient(rgba(255,255,255,0.07) 1px, transparent 1px), linear-gradient(90deg, rgba(255,255,255,0.07) 1px, transparent 1px), linear-gradient(rgba(255,255,255,0.04) 1px, transparent 1px), linear-gradient(90deg, rgba(255,255,255,0.04) 1px, transparent 1px)`, backgroundSize: `200px 200px, 200px 200px, 40px 40px, 40px 40px`, backgroundPosition: `-40px -40px` }} />
      <AbsoluteFill style={{ background: `radial-gradient(ellipse 80% 70% at 50% 50%, transparent 50%, rgba(0,0,0,0.7))` }} />

      {/* header row */}
      <div style={{ position: `absolute`, left: 72, top: 48, fontSize: 18, letterSpacing: `0.22em`, color: INK }}>
        <Decode text="EXPONENTIAL / AUTOPILOT / SYSTEM DIAGRAM / REV 2026.09" f={f} at={2} dur={48} />
      </div>
      <div style={{ position: `absolute`, right: 72, top: 48, fontSize: 18, letterSpacing: `0.12em`, color: C.muted, opacity: seg(f, 10, 20) }}>
        TC {timecode(f)}
      </div>
      <div style={{ position: `absolute`, left: 72, bottom: 48, fontSize: 15, letterSpacing: `0.2em`, color: C.dim, opacity: seg(f, 14, 26) }}>
        SHEET 1 / 1 · SCALE 1:1 · CANVAS 1920×1080
      </div>

      <svg style={{ position: `absolute`, inset: 0, opacity: diagramO }} width={1920} height={1080}>
        {/* corner crosshairs */}
        <g opacity={seg(f, 6, 20)}>
          {cross(72, 130)}
          {cross(1848, 130)}
          {cross(72, 980)}
          {cross(1848, 980)}
        </g>

        {/* devices */}
        {DEVICES.map((d) => (
          <Device key={d.id} d={d} f={f} />
        ))}

        {/* connectors hub → devices: static draw + flowing packets */}
        {DEVICES.map((d, i) => {
          const a = HUB
          const b = { x: d.x, y: d.y + (d.id === `desktop` ? 95 : d.id === `web` ? 90 : 110) }
          const c = { x: (a.x + b.x) / 2, y: 470 }
          const t = linksT(i)
          return (
            <g key={d.id}>
              <path d={qPath(a, c, b)} fill="none" stroke={LINE} strokeWidth={1.5} {...draw(t)} />
              {f >= 180 ? <path d={qPath(a, c, b)} fill="none" stroke={G} strokeWidth={3} strokeDasharray="8 44" strokeDashoffset={flow} strokeLinecap="round" opacity={0.85 * seg(f, 180, 196)} /> : null}
            </g>
          )
        })}

        {/* hub */}
        <circle cx={HUB.x} cy={HUB.y} r={82} fill="#050507" stroke={INK} strokeWidth={2} {...draw(hubT)} />
        <circle cx={HUB.x} cy={HUB.y} r={70} fill="none" stroke={FAINT} strokeWidth={1} {...draw(seg(f, 126, 150))} />
        <circle cx={HUB.x} cy={HUB.y} r={94 + 6 * Math.sin(f / 4)} fill="none" stroke={G} strokeWidth={1} opacity={f >= 180 ? 0.35 : 0} />

        {/* agent block */}
        <rect x={AGENT.x - 150} y={AGENT.y - 70} width={300} height={140} rx={10} fill="#050507" stroke={INK} strokeWidth={2} {...draw(agentT)} />
        <line x1={AGENT.x - 150} y1={AGENT.y - 34} x2={AGENT.x + 150} y2={AGENT.y - 34} stroke={FAINT} strokeWidth={1} {...draw(seg(f, 240, 256))} />
        {/* github block */}
        <rect x={GITHUB.x - 150} y={GITHUB.y - 70} width={300} height={140} rx={10} fill="#050507" stroke={INK} strokeWidth={2} {...draw(ghT)} />
        <line x1={GITHUB.x - 150} y1={GITHUB.y - 34} x2={GITHUB.x + 150} y2={GITHUB.y - 34} stroke={FAINT} strokeWidth={1} {...draw(seg(f, 250, 266))} />

        {/* the loop's three legs */}
        {[HUB_TO_AGENT, AGENT_TO_GITHUB, GITHUB_TO_HUB].map((leg, i) => (
          <g key={i}>
            <path d={qPath(leg.a, leg.c, leg.b)} fill="none" stroke={LINE} strokeWidth={1.5} {...draw(loopT(i))} />
            {f >= 290 ? <path d={qPath(leg.a, leg.c, leg.b)} fill="none" stroke={G} strokeWidth={2} strokeDasharray="6 40" strokeDashoffset={-f * 2} strokeLinecap="round" opacity={0.5 * seg(f, 290, 304)} /> : null}
          </g>
        ))}
        {/* arrowheads */}
        {[
          { p: HUB_TO_AGENT.b, r: 180 },
          { p: AGENT_TO_GITHUB.b, r: -90 },
          { p: GITHUB_TO_HUB.b, r: 180 },
        ].map((a, i) => (
          <path key={i} d="M -12 -7 L 0 0 L -12 7" fill="none" stroke={INK} strokeWidth={2} strokeLinecap="round" transform={`translate(${a.p.x} ${a.p.y}) rotate(${a.r})`} opacity={loopT(i)} />
        ))}

        {/* the highlighted packet */}
        {packet ? (
          <g>
            <circle cx={packet.x} cy={packet.y} r={22} fill={G} opacity={0.18} />
            <circle cx={packet.x} cy={packet.y} r={9} fill={G} />
          </g>
        ) : null}
        {/* arrival rings */}
        {[
          { at: 352, p: HUB_TO_AGENT.b },
          { at: 378, p: AGENT_TO_GITHUB.b },
          { at: 400, p: GITHUB_TO_HUB.b },
        ].map((r, i) => {
          const t = seg(f, r.at, r.at + 20)
          return f >= r.at && t < 1 ? <circle key={i} cx={r.p.x} cy={r.p.y} r={10 + 70 * t} fill="none" stroke={G} strokeWidth={2} opacity={1 - t} /> : null
        })}
      </svg>

      {/* HTML labels over the drawing */}
      <div style={{ position: `absolute`, inset: 0, opacity: diagramO, fontSize: 15, letterSpacing: `0.18em` }}>
        {DEVICES.map((d) => (
          <div key={d.id} style={{ position: `absolute`, left: d.x - 200, width: 400, top: d.y + 130, textAlign: `center`, color: INK }}>
            <Decode text={d.label} f={f} at={d.at + 30} dur={22} />
          </div>
        ))}
        <div style={{ position: `absolute`, left: HUB.x - 200, width: 400, top: HUB.y - 36, textAlign: `center`, color: INK, fontSize: 17 }}>
          <Decode text="POSTGRES → ELECTRIC" f={f} at={140} dur={22} />
        </div>
        <div style={{ position: `absolute`, left: HUB.x - 200, width: 400, top: HUB.y + 2, textAlign: `center`, color: G, fontSize: 30, fontWeight: 600, letterSpacing: `0.04em`, opacity: seg(f, 140, 150) }}>
          {String(shapes).padStart(2, `0`)} <span style={{ fontSize: 14, letterSpacing: `0.2em`, color: C.muted }}>SHAPES</span>
        </div>
        <div style={{ position: `absolute`, left: HUB.x - 200, width: 400, top: HUB.y + 108, textAlign: `center`, color: C.muted, fontSize: 14 }}>
          <Decode text="MEMBER-ONLY · REALTIME · ×4" f={f} at={188} dur={22} />
        </div>

        <div style={{ position: `absolute`, left: AGENT.x - 150, width: 300, top: AGENT.y - 60, textAlign: `center`, color: INK, fontSize: 16 }}>
          <Decode text="AGENT · UNATTENDED" f={f} at={244} dur={20} />
        </div>
        <div style={{ position: `absolute`, left: AGENT.x - 150, width: 300, top: AGENT.y - 12, textAlign: `center`, color: C.muted, fontSize: 14, lineHeight: `26px` }}>
          <Decode text="SCHEDULE · EVENT · WORKFLOW" f={f} at={256} dur={18} />
          <br />
          <span style={{ color: G }}>
            <Decode text="0 CLOUD AGENTS · 0 HANDS" f={f} at={266} dur={18} />
          </span>
        </div>

        <div style={{ position: `absolute`, left: GITHUB.x - 150, width: 300, top: GITHUB.y - 60, textAlign: `center`, color: INK, fontSize: 16 }}>
          <Decode text="GITHUB · PULL REQUEST" f={f} at={254} dur={20} />
        </div>
        <div style={{ position: `absolute`, left: GITHUB.x - 150, width: 300, top: GITHUB.y - 12, textAlign: `center`, color: C.muted, fontSize: 14, lineHeight: `26px` }}>
          <Decode text="exp/EXP-1100 · #916" f={f} at={266} dur={18} />
          <br />
          <span style={{ color: G }}>
            <Decode text="AUTO-MERGE → RELEASE" f={f} at={276} dur={18} />
          </span>
        </div>

        {/* leg annotations */}
        <div style={{ position: `absolute`, left: 520, top: 548, color: C.muted, fontSize: 13, opacity: loopT(0) }}>
          <Decode text="ISSUE → RUN · NO HANDS" f={f} at={262} dur={14} />
        </div>
        <div style={{ position: `absolute`, left: 880, top: 792, color: C.muted, fontSize: 13, opacity: loopT(1) }}>
          <Decode text="COMMIT · PUSH · OPEN PR" f={f} at={270} dur={16} />
        </div>
        <div style={{ position: `absolute`, left: 1220, top: 548, color: C.muted, fontSize: 13, opacity: loopT(2) }}>
          <Decode text="SHIPPED → STATUS" f={f} at={278} dur={14} />
        </div>

        {/* status ticker */}
        <div style={{ position: `absolute`, left: 0, right: 0, top: 940, display: `flex`, justifyContent: `center`, alignItems: `center`, gap: 34, ...enter(f, 300, 16, { rise: 12, blur: 6 }) }}>
          {STATUS_STEPS.map((s, i) => {
            const active = i === stepIdx
            const past = i < stepIdx
            return (
              <React.Fragment key={s.kind}>
                <span style={{ display: `inline-flex`, alignItems: `center`, gap: 10, color: active ? INK : past ? C.muted : C.dim, scale: String(active ? 1 + 0.2 * (1 - stepPop) : 1) }}>
                  <StatusGlyph kind={s.kind} size={20} color={active || past ? undefined : C.dim} />
                  <span style={{ fontSize: 15, letterSpacing: `0.22em`, fontWeight: active ? 600 : 400 }}>{s.label}</span>
                </span>
                {i < STATUS_STEPS.length - 1 ? <span style={{ color: C.dim }}>→</span> : null}
              </React.Fragment>
            )
          })}
        </div>
      </div>

      {/* brand as the last stroke */}
      {f >= 410 ? <BrandCard f={f} at={410} mono sub="exponential.at · AUTOMATE SOFTWARE END TO END" /> : null}

      {/* scanlines + a sweeping scan line */}
      <AbsoluteFill style={{ backgroundImage: `repeating-linear-gradient(to bottom, rgba(255,255,255,0.05) 0px, rgba(255,255,255,0.05) 1px, transparent 1px, transparent 3px)`, pointerEvents: `none`, opacity: 0.6 }} />
      <div style={{ position: `absolute`, left: 0, right: 0, top: lerp(-40, 1120, (f % 150) / 150), height: 40, background: `linear-gradient(to bottom, transparent, rgba(34,197,94,0.06), transparent)`, pointerEvents: `none` }} />
      <div style={{ position: `absolute`, left: 0, bottom: 0, height: 3, width: (f / 450) * 1920, background: G, opacity: 0.7 }} />
    </AbsoluteFill>
  )
}
