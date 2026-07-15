// spot/footage.tsx — Seedance live-action layer: the clip manifest, the graded
// <FootageClip> and the placeholder slate rendered until a take lands.
// Files live in public/footage/ (gitignored); flip `ready` when a pick exists.

import React from "react"
import { AbsoluteFill, OffthreadVideo, Sequence, interpolate, staticFile, useCurrentFrame } from "remotion"
import { C, EASE, MONO_FONT, UI_FONT } from "../ships/theme"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

export type FootageSpec = {
  file: string // path under public/
  ready: boolean // false ⇒ placeholder slate
  trimBeforeSec: number // seconds into the source take
  label: string // slate headline
  desc: string[] // slate shot description
  // Vertical crop anchor (object-position y%) for non-16:9 takes — the picked
  // takes are 960×960, so cover-crop keeps only the middle 56%: seated shots
  // need a high anchor (face in the upper third), typing macros a centered one.
  // A function gets the sequence-LOCAL frame (per-shot anchor jumps hide inside
  // the take's own hard cuts).
  objectPosY?: number | ((localFrame: number) => number)
}

// The manifest, tuned to the PICKED takes (2026-07-13: cafe-a 11.0s, cafe-c
// 10.1s — both 960×960 @24fps). cafe-a is ONE generation used twice: shots are
// sit ~0–2s · typing macro ~2–6.5s · over-shoulder screen push ~6.5–11s.
export const FOOTAGE = {
  chaos: {
    file: "footage/cafe-a.mp4",
    ready: true,
    // 0.8: the take fades its ambience in from silence — open mid-atmosphere
    // (and mid-motion: a better cold open than the shot's actual first frames)
    trimBeforeSec: 0.8,
    label: "LIVE — CAFÉ CHAOS",
    desc: [
      "He sits down at the window table, opens the MacBook (screen away).",
      "Crowded café: baristas steaming milk, chatter, cups. LOUD.",
    ],
    // sit shot (face high) until the take's own cut to the typing macro at
    // source ~2.0s = local (2.0 − trim 0.8) × 30 ≈ f36
    objectPosY: (f: number) => interpolate(f, [32, 40], [16, 50], CLAMP),
  },
  typing: {
    file: "footage/cafe-a.mp4",
    ready: true,
    // = chaos trim (0.8) + segment-A length (4.0): source time stays CONTINUOUS
    // across the A→B cut (both sides sit inside the same typing shot)
    trimBeforeSec: 4.8,
    label: "LIVE — THE QUIET",
    desc: [
      "Macro: fingers start typing, calm and deliberate.",
      "Push-in over the shoulder — screen is a bokeh glow.",
      "Ambience fades until only keystrokes remain.",
    ],
    objectPosY: 50,
  },
  payoff: {
    file: "footage/cafe-c.mp4",
    ready: true,
    trimBeforeSec: 0.2,
    label: "LIVE — BEFORE THE COFFEE",
    desc: [
      "A flat white lands on the wood next to the laptop. Clink.",
      "He leans back, half-closes the lid, small smile,",
      "nods thanks to the barista, picks up the cup.",
    ],
    objectPosY: 30, // one anchor fits both the cup close-up and the seated shots
  },
} satisfies Record<string, FootageSpec>

// ── Placeholder slate (deterministic; shows the cut is alive pre-footage) ─────
const Slate: React.FC<{ spec: FootageSpec; duration: number }> = ({ spec, duration }) => {
  const frame = useCurrentFrame() // sequence-local
  const sweep = interpolate(frame % 90, [0, 90], [-0.25, 1.25])
  return (
    <AbsoluteFill style={{ backgroundColor: "#0b0b0d", alignItems: "center", justifyContent: "center" }}>
      <div
        style={{
          position: "absolute",
          inset: 24,
          border: `1px dashed rgba(255,255,255,0.18)`,
          borderRadius: 12,
        }}
      />
      {/* shimmer bar — a stand-in for motion */}
      <div
        style={{
          position: "absolute",
          left: 0,
          right: 0,
          top: 0,
          height: 3,
          background: `linear-gradient(to right, transparent ${(sweep - 0.12) * 100}%, ${C.indigoGlow} ${sweep * 100}%, transparent ${(sweep + 0.12) * 100}%)`,
          opacity: 0.7,
        }}
      />
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 18, maxWidth: 900, textAlign: "center" }}>
        <div style={{ fontFamily: MONO_FONT, fontSize: 13, letterSpacing: 4, color: C.indigoGlow }}>
          SEEDANCE TAKE PENDING
        </div>
        <div style={{ fontFamily: UI_FONT, fontSize: 44, fontWeight: 700, letterSpacing: -1, color: C.text }}>
          {spec.label}
        </div>
        <div style={{ fontFamily: UI_FONT, fontSize: 19, lineHeight: 1.5, color: C.muted }}>
          {spec.desc.map((l) => (
            <div key={l}>{l}</div>
          ))}
        </div>
        <div style={{ fontFamily: MONO_FONT, fontSize: 13, color: C.dim, marginTop: 10 }}>
          {spec.file} · trim {spec.trimBeforeSec.toFixed(1)}s · {(duration / 30).toFixed(1)}s used · {frame}f
        </div>
      </div>
    </AbsoluteFill>
  )
}

// ── Graded footage clip ───────────────────────────────────────────────────────
// Sits ABOVE the UI/camera layer; fadeOut reveals what's underneath (the B→U1
// crossfade). volume takes sequence-local frames (the §5 mix automation).
export const FootageClip: React.FC<{
  spec: FootageSpec
  from: number
  duration: number
  fadeIn?: number
  fadeOut?: number
  volume?: (localFrame: number) => number
}> = ({ spec, from, duration, fadeIn = 0, fadeOut = 0, volume }) => {
  const frame = useCurrentFrame() // composition-global
  if (frame < from || frame >= from + duration) return null
  const local = frame - from
  const o =
    interpolate(local, [0, Math.max(fadeIn, 0.01)], [fadeIn > 0 ? 0 : 1, 1], { ...CLAMP, easing: EASE }) *
    interpolate(local, [duration - Math.max(fadeOut, 0.01), duration], [1, fadeOut > 0 ? 0 : 1], { ...CLAMP, easing: EASE })
  const posY = typeof spec.objectPosY === "function" ? spec.objectPosY(local) : (spec.objectPosY ?? 50)
  return (
    <AbsoluteFill style={{ opacity: o }}>
      <Sequence from={from} durationInFrames={duration}>
        {spec.ready ? (
          <>
            <OffthreadVideo
              src={staticFile(spec.file)}
              trimBefore={Math.round(spec.trimBeforeSec * 30)}
              volume={volume ?? 1}
              style={{
                width: "100%",
                height: "100%",
                objectFit: "cover",
                objectPosition: `50% ${posY}%`,
                // sit the naturalistic footage on the dark-indigo brand canvas
                filter: "saturate(0.92) contrast(1.04)",
              }}
            />
            {/* vignette — continuity of light with the glow canvas */}
            <AbsoluteFill
              style={{
                background: "radial-gradient(ellipse at 50% 46%, transparent 58%, rgba(9,9,11,0.55) 100%)",
              }}
            />
          </>
        ) : (
          <Slate spec={spec} duration={duration} />
        )}
      </Sequence>
    </AbsoluteFill>
  )
}
