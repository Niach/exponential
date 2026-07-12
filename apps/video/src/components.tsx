import { AbsoluteFill, Easing, Img, interpolate, staticFile, useCurrentFrame } from "remotion"
import { COLORS } from "./theme"

const EASE = Easing.bezier(0.16, 1, 0.3, 1)

// Dark background with a soft indigo radial glow that drifts slightly.
export const Background: React.FC = () => {
  const frame = useCurrentFrame()
  const drift = interpolate(frame, [0, 300], [0, 40], {
    extrapolateRight: "clamp",
  })
  return (
    <AbsoluteFill style={{ backgroundColor: COLORS.bg }}>
      <AbsoluteFill
        style={{
          background: `radial-gradient(720px 520px at ${50 + drift / 10}% 32%, rgba(99,102,241,0.20), transparent 70%)`,
        }}
      />
      <AbsoluteFill
        style={{
          background: `radial-gradient(600px 400px at 88% 92%, rgba(129,140,248,0.10), transparent 70%)`,
        }}
      />
    </AbsoluteFill>
  )
}

// Striped-sphere brand mark, approximating the Exponential logo.
export const Logo: React.FC<{ size: number }> = ({ size }) => {
  return (
    <svg width={size} height={size} viewBox="0 0 100 100">
      <defs>
        <clipPath id="sphere">
          <circle cx="50" cy="50" r="47" />
        </clipPath>
        <linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#fafafa" />
          <stop offset="1" stopColor="#c7c7d1" />
        </linearGradient>
      </defs>
      <circle cx="50" cy="50" r="47" fill="url(#g)" />
      <g clipPath="url(#sphere)" fill={COLORS.bg} opacity="0.9">
        <path d="M -20 26 Q 50 8 120 26 L 120 34 Q 50 18 -20 38 Z" />
        <path d="M -20 48 Q 50 32 120 46 L 120 55 Q 50 42 -20 60 Z" />
        <path d="M -20 70 Q 50 56 120 68 L 120 78 Q 50 66 -20 82 Z" />
      </g>
    </svg>
  )
}

// A browser-window mockup wrapping a screenshot.
export const BrowserWindow: React.FC<{
  src: string
  url: string
  width: number
  zoom?: number
  frame: number
}> = ({ src, url, width, zoom = 1, frame }) => {
  const barH = Math.round(width * 0.03)
  return (
    <div
      style={{
        width,
        borderRadius: 16,
        overflow: "hidden",
        border: `1px solid ${COLORS.border}`,
        boxShadow: "0 40px 120px rgba(0,0,0,0.6), 0 8px 24px rgba(0,0,0,0.4)",
        backgroundColor: COLORS.panel,
      }}
    >
      <div
        style={{
          height: barH,
          backgroundColor: COLORS.panel,
          display: "flex",
          alignItems: "center",
          gap: barH * 0.28,
          paddingLeft: barH * 0.5,
          borderBottom: `1px solid ${COLORS.border}`,
        }}
      >
        {["#ff5f57", "#febc2e", "#28c840"].map((c) => (
          <div
            key={c}
            style={{
              width: barH * 0.28,
              height: barH * 0.28,
              borderRadius: "50%",
              backgroundColor: c,
              opacity: 0.85,
            }}
          />
        ))}
        <div
          style={{
            marginLeft: barH * 0.6,
            marginRight: "auto",
            height: barH * 0.58,
            minWidth: width * 0.32,
            borderRadius: barH,
            backgroundColor: "rgba(255,255,255,0.05)",
            color: COLORS.textMuted,
            fontSize: barH * 0.36,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            paddingInline: barH,
          }}
        >
          {url}
        </div>
      </div>
      <div style={{ overflow: "hidden", lineHeight: 0 }}>
        <Img
          src={staticFile(src)}
          style={{
            width: "100%",
            display: "block",
            scale: String(zoom),
            transformOrigin: "center top",
          }}
        />
      </div>
    </div>
  )
}

// Wraps a scene with a gentle fade + rise in, and fade out near the end.
export const Scene: React.FC<{
  durationInFrames: number
  children: React.ReactNode
}> = ({ durationInFrames, children }) => {
  const frame = useCurrentFrame()
  const opacity = interpolate(
    frame,
    [0, 14, durationInFrames - 12, durationInFrames],
    [0, 1, 1, 0],
    { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: EASE }
  )
  return (
    <AbsoluteFill style={{ opacity }}>{children}</AbsoluteFill>
  )
}

export { EASE }
