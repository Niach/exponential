// acts/Everywhere.tsx — the four clients (8–11.3 s): desktop IDE, web,
// iPhone and Android fly in through a shared perspective, each showing the
// same board. The desktop flips the hero issue to review; sync pulses race
// along arcs to the other three, which flip the same frame they land. The
// devices scatter back out along their entry vectors for the ship act.

import React from "react"
import { AbsoluteFill } from "remotion"
import { C, MONO, UI, VIOLET } from "../theme"
import { IN, SETTLE, enter, lerp, pop, seg, stagger } from "../motion"
import { Caption, IssueRow, Ripple, StatusGlyph, type StatusKind } from "../atoms"

type Kind = `desktop` | `web` | `ios` | `android`
type Device = {
  kind: Kind
  cx: number
  cy: number
  w: number
  h: number
  from: { x: number; y: number; ry: number }
  at: number
  label: string
  chrome: number
  pad: number
  radius: number
}

const DEVICES: Device[] = [
  { kind: `desktop`, cx: 470, cy: 520, w: 660, h: 430, from: { x: -420, y: 40, ry: 38 }, at: 0, label: `macOS · Windows · Linux`, chrome: 30, pad: 0, radius: 12 },
  { kind: `web`, cx: 1030, cy: 500, w: 420, h: 310, from: { x: 0, y: -360, ry: -18 }, at: 6, label: `Web`, chrome: 38, pad: 0, radius: 12 },
  { kind: `ios`, cx: 1410, cy: 540, w: 212, h: 440, from: { x: 60, y: 380, ry: -10 }, at: 12, label: `iOS`, chrome: 40, pad: 10, radius: 38 },
  { kind: `android`, cx: 1690, cy: 540, w: 212, h: 440, from: { x: 420, y: 60, ry: -38 }, at: 18, label: `Android`, chrome: 40, pad: 10, radius: 30 },
]

const DESKTOP_FLIP = 40
const OTHERS_FLIP = 54
const EXIT_AT = 84

// Where each device's hero glyph sits, in comp px (MiniBoard geometry).
const RAIL: Record<Kind, number> = { desktop: 44, web: 0, ios: 0, android: 0 }
const MB_HEADER = 32
const MB_BAND = 24
const MB_ROW = 34

const heroGlyphAt = (d: Device) => {
  const left = d.cx - d.w / 2 + d.pad + RAIL[d.kind]
  const top = d.cy - d.h / 2 + d.pad + d.chrome
  return { x: left + 12 + 8, y: top + MB_HEADER + MB_BAND + MB_ROW / 2 }
}

const MiniBoard: React.FC<{ f: number; flipAt: number; hero: StatusKind; kind: Kind }> = ({ f, flipAt, hero, kind }) => {
  const flipped = f >= flipAt
  const scalePop = flipped ? 1 + 0.5 * (1 - seg(f, flipAt, flipAt + 10)) : 1
  const rows: { ident: string; title: string; status: StatusKind; hue: number | null }[] = [
    { ident: `EXP-1100`, title: `Ship the 15-second showreel`, status: flipped ? `review` : hero, hue: 6 },
    { ident: `EXP-1097`, title: `Review wave clears its layer`, status: `progress`, hue: 4 },
    { ident: `EXP-1102`, title: `Workflow host state on the device`, status: `backlog`, hue: null },
  ]
  const phone = kind === `ios` || kind === `android`
  return (
    <div style={{ position: `absolute`, inset: 0, display: `flex` }}>
      {kind === `desktop` ? (
        <div style={{ width: RAIL.desktop, borderRight: `1px solid ${C.strokeRow}`, background: C.fillSection, display: `flex`, flexDirection: `column`, alignItems: `center`, gap: 10, paddingTop: 12 }}>
          {[VIOLET, C.strokeStrong, C.strokeStrong, C.strokeStrong].map((c, i) => (
            <span key={i} style={{ width: 16, height: 16, borderRadius: 5, background: c }} />
          ))}
        </div>
      ) : null}
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: `flex`, alignItems: `center`, gap: 8, height: MB_HEADER, padding: `0 12px`, fontFamily: UI, fontSize: phone ? 12 : 13, fontWeight: 600, color: C.text, borderBottom: `1px solid ${C.strokeRow}` }}>
          <span style={{ width: 8, height: 8, borderRadius: 3, background: VIOLET }} />
          Product
          <span style={{ flex: 1 }} />
          <span style={{ width: 6, height: 6, borderRadius: `50%`, background: C.green }} />
        </div>
        <div style={{ display: `flex`, alignItems: `center`, gap: 6, height: MB_BAND, padding: `0 12px`, background: `${C.statusInProgress}1a`, fontFamily: UI, fontSize: 11, fontWeight: 600, color: C.text }}>
          <StatusGlyph kind="progress" size={11} />
          In progress
        </div>
        {rows.map((r, i) => (
          <IssueRow
            key={r.ident}
            ident={phone ? `` : r.ident}
            title={r.title}
            status={r.status}
            prio={i === 0 ? 3 : i === 1 ? 2 : 1}
            assignee={r.hue === null ? null : { initials: r.hue === 6 ? `DS` : `MK`, hue: r.hue }}
            h={MB_ROW}
            font={phone ? 11.5 : 12.5}
            glyph={16}
            style={i === 0 && flipped ? { background: `rgba(34,197,94,0.06)` } : undefined}
            trailing={
              i === 0 ? (
                <span style={{ display: `inline-block`, scale: String(scalePop) }} />
              ) : undefined
            }
          />
        ))}
      </div>
    </div>
  )
}

const Frame: React.FC<{ d: Device; f: number; flipAt: number }> = ({ d, f, flipAt }) => {
  const s = pop(f, d.at, SETTLE)
  const ex = seg(f, EXIT_AT, EXIT_AT + 12, IN)
  const x = lerp(d.from.x, 0, s) + d.from.x * 0.7 * ex
  const y = lerp(d.from.y, 0, s) + d.from.y * 0.7 * ex
  const ry = lerp(d.from.ry, 0, s) - d.from.ry * 0.6 * ex
  const o = Math.min(1, s * 1.4) * (1 - ex)
  const blur = (1 - Math.min(1, s * 1.2)) * 14 + ex * 14
  const phone = d.kind === `ios` || d.kind === `android`
  return (
    <div
      style={{
        position: `absolute`,
        left: d.cx - d.w / 2,
        top: d.cy - d.h / 2,
        width: d.w,
        height: d.h,
        transform: `translate(${x}px, ${y}px) rotateY(${ry}deg)`,
        opacity: o,
        filter: blur > 0.3 ? `blur(${blur}px)` : undefined,
        borderRadius: d.radius,
        background: phone ? `#141417` : C.canvas,
        border: `1px solid ${phone ? C.strokeStrong : C.strokeCard}`,
        boxShadow: `0 40px 100px -20px rgba(0,0,0,0.8), inset 0 1px 0 rgba(255,255,255,0.08)`,
        overflow: `hidden`,
      }}
    >
      {/* chrome */}
      {d.kind === `desktop` ? (
        <div style={{ position: `absolute`, left: 0, right: 0, top: 0, height: d.chrome, display: `flex`, alignItems: `center`, gap: 8, padding: `0 12px`, background: C.fillSection, borderBottom: `1px solid ${C.strokeRow}` }}>
          {[`#ff5f57`, `#febc2e`, `#28c840`].map((c) => (
            <span key={c} style={{ width: 11, height: 11, borderRadius: `50%`, background: c }} />
          ))}
          <span style={{ flex: 1 }} />
          <span style={{ fontFamily: UI, fontSize: 12, color: C.dim }}>Exponential</span>
          <span style={{ flex: 1 }} />
        </div>
      ) : d.kind === `web` ? (
        <div style={{ position: `absolute`, left: 0, right: 0, top: 0, height: d.chrome, display: `flex`, alignItems: `center`, gap: 10, padding: `0 12px`, background: C.fillSection, borderBottom: `1px solid ${C.strokeRow}` }}>
          {[`#ff5f57`, `#febc2e`, `#28c840`].map((c) => (
            <span key={c} style={{ width: 10, height: 10, borderRadius: `50%`, background: c }} />
          ))}
          <span style={{ flex: 1, height: 22, borderRadius: 6, background: C.fillRow, border: `1px solid ${C.strokeRow}`, fontFamily: MONO, fontSize: 11, color: C.muted, display: `flex`, alignItems: `center`, paddingLeft: 8 }}>
            app.exponential.at/t/acme/boards/product
          </span>
        </div>
      ) : (
        <div style={{ position: `absolute`, left: d.pad, right: d.pad, top: d.pad, height: d.chrome, display: `flex`, alignItems: `center`, justifyContent: `space-between`, padding: `0 16px`, fontFamily: UI, fontSize: 12, fontWeight: 600, color: C.text }}>
          <span>9:41</span>
          {d.kind === `ios` ? (
            <span style={{ width: 62, height: 18, borderRadius: 9, background: `#000`, position: `absolute`, left: `50%`, top: 6, translate: `-50% 0` }} />
          ) : (
            <span style={{ width: 12, height: 12, borderRadius: `50%`, background: `#000`, position: `absolute`, left: `50%`, top: 8, translate: `-50% 0` }} />
          )}
          <span style={{ display: `flex`, gap: 3 }}>
            {[6, 9, 12].map((h) => (
              <span key={h} style={{ width: 3, height: h, background: C.text, borderRadius: 1, alignSelf: `flex-end` }} />
            ))}
          </span>
        </div>
      )}
      {/* screen */}
      <div style={{ position: `absolute`, left: d.pad, right: d.pad, top: d.pad + d.chrome, bottom: d.pad, background: phone ? C.canvas : undefined, borderRadius: phone ? d.radius - d.pad : 0, overflow: `hidden` }}>
        <MiniBoard f={f} flipAt={flipAt} hero="progress" kind={d.kind} />
        {phone ? (
          <span style={{ position: `absolute`, left: `50%`, bottom: 8, translate: `-50% 0`, width: 80, height: 4, borderRadius: 2, background: C.strokeActive }} />
        ) : null}
      </div>
    </div>
  )
}

// Quadratic arc from a to b, bowed upward.
const arc = (a: { x: number; y: number }, b: { x: number; y: number }) => {
  const mx = (a.x + b.x) / 2
  const my = Math.min(a.y, b.y) - 150 - Math.abs(b.x - a.x) * 0.12
  return { c: { x: mx, y: my }, d: `M ${a.x} ${a.y} Q ${mx} ${my} ${b.x} ${b.y}` }
}
const onArc = (a: { x: number; y: number }, c: { x: number; y: number }, b: { x: number; y: number }, t: number) => ({
  x: (1 - t) ** 2 * a.x + 2 * (1 - t) * t * c.x + t ** 2 * b.x,
  y: (1 - t) ** 2 * a.y + 2 * (1 - t) * t * c.y + t ** 2 * b.y,
})

export const Everywhere: React.FC<{ l: number }> = ({ l }) => {
  const src = heroGlyphAt(DEVICES[0])
  const targets = DEVICES.slice(1).map(heroGlyphAt)
  const pulseT = seg(l, DESKTOP_FLIP + 2, OTHERS_FLIP)
  const exitO = 1 - seg(l, EXIT_AT + 2, EXIT_AT + 12, IN)

  return (
    <AbsoluteFill>
      <div style={{ position: `absolute`, inset: 0, perspective: 1800, perspectiveOrigin: `50% 45%` }}>
        {DEVICES.map((d, i) => (
          <Frame key={d.kind} d={d} f={l} flipAt={i === 0 ? DESKTOP_FLIP : OTHERS_FLIP} />
        ))}
      </div>

      {/* sync arcs + travelling pulses */}
      <svg style={{ position: `absolute`, inset: 0, pointerEvents: `none`, opacity: exitO }} width={1920} height={1080}>
        {targets.map((t, i) => {
          const { c, d } = arc(src, t)
          const draw = seg(l, DESKTOP_FLIP + 2 + i * 2, OTHERS_FLIP)
          const fade = 1 - seg(l, OTHERS_FLIP + 6, OTHERS_FLIP + 22)
          const p = onArc(src, c, t, pulseT)
          return (
            <g key={i}>
              <path d={d} fill="none" stroke={C.green} strokeWidth={1.5} pathLength={1} strokeDasharray={1} strokeDashoffset={1 - draw} opacity={0.5 * fade} />
              {pulseT > 0 && pulseT < 1 ? (
                <>
                  <circle cx={p.x} cy={p.y} r={14} fill={C.green} opacity={0.25} />
                  <circle cx={p.x} cy={p.y} r={6} fill={C.green} />
                </>
              ) : null}
            </g>
          )
        })}
      </svg>
      <Ripple f={l} at={DESKTOP_FLIP} x={src.x} y={src.y} size={90} color={C.statusInReview} />
      {targets.map((t, i) => (
        <Ripple key={i} f={l} at={OTHERS_FLIP} x={t.x} y={t.y} size={80} color={C.statusInReview} />
      ))}

      {/* labels */}
      {DEVICES.map((d, i) => (
        <div
          key={d.kind}
          style={{
            position: `absolute`,
            left: d.cx - 200,
            width: 400,
            top: d.cy + d.h / 2 + 26,
            textAlign: `center`,
            fontFamily: MONO,
            fontSize: 16,
            letterSpacing: `0.14em`,
            textTransform: `uppercase`,
            color: C.muted,
            opacity: exitO,
          }}
        >
          <span style={{ display: `inline-block`, ...enter(l, stagger(22, i, 5), 14, { rise: 12, blur: 6 }) }}>{d.label}</span>
        </div>
      ))}

      <Caption f={l} at={24} out={80} index="03 / Everywhere" line="One sync. Native on every platform." />
    </AbsoluteFill>
  )
}
