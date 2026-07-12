import {
  AbsoluteFill,
  interpolate,
  Sequence,
  useCurrentFrame,
} from "remotion"
import { Background, BrowserWindow, EASE, Logo, Scene } from "./components"
import { COLORS, fontFamily } from "./theme"

const URL = "app.exponential.at"

const Kicker: React.FC<{ children: React.ReactNode; delay?: number }> = ({
  children,
  delay = 0,
}) => {
  const frame = useCurrentFrame()
  const o = interpolate(frame, [delay, delay + 16], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: EASE,
  })
  return (
    <div
      style={{
        opacity: o,
        color: COLORS.accentSoft,
        fontSize: 28,
        fontWeight: 600,
        letterSpacing: 4,
        textTransform: "uppercase",
      }}
    >
      {children}
    </div>
  )
}

const Headline: React.FC<{ children: React.ReactNode; delay?: number }> = ({
  children,
  delay = 4,
}) => {
  const frame = useCurrentFrame()
  const o = interpolate(frame, [delay, delay + 20], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: EASE,
  })
  const y = interpolate(frame, [delay, delay + 20], [16, 0], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: EASE,
  })
  return (
    <div
      style={{
        opacity: o,
        translate: `0px ${y}px`,
        color: COLORS.text,
        fontSize: 76,
        fontWeight: 700,
        letterSpacing: -1.5,
        textAlign: "center",
      }}
    >
      {children}
    </div>
  )
}

// A screenshot scene: headline on top, browser window rising in below.
const ShotScene: React.FC<{
  kicker: string
  title: string
  src: string
  width?: number
  zoom?: number
}> = ({ kicker, title, src, width = 1240, zoom = 1 }) => {
  const frame = useCurrentFrame()
  const y = interpolate(frame, [6, 30], [46, 0], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: EASE,
  })
  const s = interpolate(frame, [6, 30], [0.97, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: EASE,
  })
  return (
    <AbsoluteFill
      style={{
        fontFamily,
        alignItems: "center",
        justifyContent: "center",
        gap: 44,
        padding: 60,
      }}
    >
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 14,
        }}
      >
        <Kicker>{kicker}</Kicker>
        <Headline>{title}</Headline>
      </div>
      <div style={{ translate: `0px ${y}px`, scale: String(s) }}>
        <BrowserWindow
          src={src}
          url={URL}
          width={width}
          zoom={zoom}
          frame={frame}
        />
      </div>
    </AbsoluteFill>
  )
}

const IntroScene: React.FC = () => {
  const frame = useCurrentFrame()
  const logoS = interpolate(frame, [0, 26], [0.6, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: EASE,
  })
  const logoO = interpolate(frame, [0, 20], [0, 1], {
    extrapolateRight: "clamp",
  })
  const wordO = interpolate(frame, [16, 36], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: EASE,
  })
  const wordX = interpolate(frame, [16, 36], [-24, 0], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: EASE,
  })
  const tagO = interpolate(frame, [34, 54], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: EASE,
  })
  return (
    <AbsoluteFill
      style={{
        fontFamily,
        alignItems: "center",
        justifyContent: "center",
        flexDirection: "column",
        gap: 30,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 28 }}>
        <div style={{ opacity: logoO, scale: String(logoS) }}>
          <Logo size={118} />
        </div>
        <div
          style={{
            opacity: wordO,
            translate: `${wordX}px 0px`,
            color: COLORS.text,
            fontSize: 108,
            fontWeight: 700,
            letterSpacing: -3,
          }}
        >
          Exponential
        </div>
      </div>
      <div
        style={{
          opacity: tagO,
          color: COLORS.textMuted,
          fontSize: 40,
          fontWeight: 500,
          letterSpacing: 0.5,
        }}
      >
        The real-time issue tracker
      </div>
    </AbsoluteFill>
  )
}

const OutroScene: React.FC = () => {
  const frame = useCurrentFrame()
  const o = interpolate(frame, [0, 22], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: EASE,
  })
  const s = interpolate(frame, [0, 30], [0.9, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: EASE,
  })
  const lineO = interpolate(frame, [24, 44], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: EASE,
  })
  return (
    <AbsoluteFill
      style={{
        fontFamily,
        alignItems: "center",
        justifyContent: "center",
        flexDirection: "column",
        gap: 28,
      }}
    >
      <div
        style={{
          opacity: o,
          scale: String(s),
          display: "flex",
          alignItems: "center",
          gap: 24,
        }}
      >
        <Logo size={78} />
        <div
          style={{
            color: COLORS.text,
            fontSize: 78,
            fontWeight: 700,
            letterSpacing: -2,
          }}
        >
          Exponential
        </div>
      </div>
      <div
        style={{
          opacity: lineO,
          color: COLORS.textMuted,
          fontSize: 30,
          fontWeight: 500,
          letterSpacing: 3,
        }}
      >
        Electric SQL · TanStack Start · React 19
      </div>
    </AbsoluteFill>
  )
}

export const WebUiDemo: React.FC = () => {
  return (
    <AbsoluteFill style={{ backgroundColor: COLORS.bg }}>
      <Background />

      <Sequence from={0} durationInFrames={80} name="Intro">
        <Scene durationInFrames={80}>
          <IntroScene />
        </Scene>
      </Sequence>

      <Sequence from={72} durationInFrames={118} name="Login">
        <Scene durationInFrames={118}>
          <ShotScene
            kicker="Sign in"
            title="One clean way in"
            src="shots/01-login.png"
            width={1180}
          />
        </Scene>
      </Sequence>

      <Sequence from={182} durationInFrames={188} name="Board">
        <Scene durationInFrames={188}>
          <ShotScene
            kicker="Issue board"
            title="Everything, grouped and live"
            src="shots/02-board.png"
            width={1300}
          />
        </Scene>
      </Sequence>

      <Sequence from={362} durationInFrames={150} name="Issue">
        <Scene durationInFrames={150}>
          <ShotScene
            kicker="Issue detail"
            title="Every detail in one place"
            src="shots/03-issue.png"
            width={1300}
          />
        </Scene>
      </Sequence>

      <Sequence from={504} durationInFrames={86} name="Outro">
        <Scene durationInFrames={86}>
          <OutroScene />
        </Scene>
      </Sequence>
    </AbsoluteFill>
  )
}
