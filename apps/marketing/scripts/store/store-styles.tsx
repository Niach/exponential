/* The three candidate store-screenshot LOOKS (EXP-566) plus the primitives they
   share. Rendered to static markup and photographed by headless Chromium — the
   reason this replaced satori is right here: real `box-shadow`, `filter` and
   `backdrop-filter`, none of which survive satori→resvg (resvg panics on the
   shadow filter at these canvas sizes).

   Coordinate space: everything below is in CSS px at HALF the output canvas, and
   the browser renders it at deviceScaleFactor 2 — so `1 CSS px = 2 output px`
   and a raw capture pixel is "native scale" when displayed at 0.5 CSS px.

   Pick the shipping set with PRODUCTION_SET in store-slides.ts. */

import type { CSSProperties, ReactNode } from "react"
import type { Rect } from "./store-crops"
import type { Slide, StyleSetId } from "./store-slides"
import type { Form } from "./raw-store"

/** Output canvas px per CSS px. Fixed: the CLI always lays out at canvas/2. */
export const CSS_SCALE = 2

/** A pop-out is never blown up past this multiple of its native pixel size. */
const MAX_POP_UPSCALE = 1.15

export type RenderInput = {
  slide: Slide
  form: Form
  /** Output canvas in real pixels. */
  canvas: { width: number; height: number }
  /** Data URI of the raw capture behind the bezel, if this slide has one. */
  shotUri: string | null
  rawWidth: number
  rawHeight: number
  pop: Rect | null
  /** Data URI of the wordmark. */
  logo: string
}

type Ctx = RenderInput & { w: number; h: number; base: number }

function ctx(input: RenderInput): Ctx {
  const w = input.canvas.width / CSS_SCALE
  const h = input.canvas.height / CSS_SCALE
  // Type scales off the SHORTER reference so the squat iPad canvas doesn't get
  // phone-proportioned headlines that eat half the screenshot (EXP-580).
  return { ...input, w, h, base: Math.min(w, Math.round(h / 2)) }
}

/* ── Primitives ──────────────────────────────────────────────────────────── */

/** CSS device bezel — a dark rounded slab with a ring. No real device art:
 *  Apple rejects listings that imply hardware they didn't licence. */
function Frame({
  screenW,
  rawWidth,
  rawHeight,
  src,
  radius,
  ring,
  bezel,
  ringColor,
  shadow,
  notch,
  style,
}: {
  screenW: number
  rawWidth: number
  rawHeight: number
  src: string | null
  radius: number
  ring: number
  bezel: string
  ringColor: string
  shadow: string
  notch: boolean
  style?: CSSProperties
}) {
  // Aspect is always preserved; if the screen box and the raw ever disagree the
  // `contain` fit letterboxes rather than stretching (the Android emulator's
  // 1080×2400 into a 1080×1920 canvas).
  const screenH = Math.round((screenW * rawHeight) / rawWidth)
  return (
    <div
      style={{
        position: `relative`,
        padding: `${ring}px`,
        borderRadius: `${radius}px`,
        background: bezel,
        boxShadow: shadow,
        outline: `1.5px solid ${ringColor}`,
        outlineOffset: `-1.5px`,
        flex: `0 0 auto`,
        ...style,
      }}
    >
      <div
        style={{
          position: `relative`,
          width: `${screenW}px`,
          height: `${screenH}px`,
          borderRadius: `${Math.max(2, radius - ring)}px`,
          overflow: `hidden`,
          background: `#050505`,
        }}
      >
        {src ? (
          <img
            src={src}
            style={{ width: `100%`, height: `100%`, objectFit: `contain`, display: `block` }}
          />
        ) : null}
        {notch ? (
          <div
            style={{
              position: `absolute`,
              top: `${Math.round(screenW * 0.022)}px`,
              left: `50%`,
              transform: `translateX(-50%)`,
              width: `${Math.round(screenW * 0.24)}px`,
              height: `${Math.round(screenW * 0.055)}px`,
              borderRadius: `9999px`,
              background: `#000`,
            }}
          />
        ) : null}
      </div>
    </div>
  )
}

/** A crop of the raw, floated over the frame at (near-)native pixel scale.
 *  Implemented with background-position/size so no extra image decode is
 *  needed and the crop stays exact. */
function PopOut({
  rect,
  rawWidth,
  rawHeight,
  src,
  targetW,
  card,
  rotate,
  radius,
  style,
}: {
  rect: Rect
  rawWidth: number
  rawHeight: number
  src: string
  targetW: number
  card: CSSProperties
  rotate: number
  radius: number
  style?: CSSProperties
}) {
  const cropPxW = rect.w * rawWidth
  const cropPxH = rect.h * rawHeight
  // Native = one raw pixel per output pixel = half a CSS px.
  const nativeW = cropPxW / CSS_SCALE
  const displayW = Math.min(targetW, nativeW * MAX_POP_UPSCALE)
  const displayH = (displayW * cropPxH) / cropPxW
  const bgW = displayW / rect.w
  const bgH = (bgW * rawHeight) / rawWidth
  return (
    <div
      style={{
        transform: `rotate(${rotate}deg)`,
        borderRadius: `${radius}px`,
        overflow: `hidden`,
        ...card,
        ...style,
      }}
    >
      <div
        style={{
          width: `${Math.round(displayW)}px`,
          height: `${Math.round(displayH)}px`,
          backgroundImage: `url(${src})`,
          backgroundSize: `${bgW}px ${bgH}px`,
          backgroundPosition: `${-rect.x * bgW}px ${-rect.y * bgH}px`,
          backgroundRepeat: `no-repeat`,
          borderRadius: `${Math.max(0, radius - 6)}px`,
        }}
      />
    </div>
  )
}

/** Headline block. `align` drives the whole type column. */
function Headline({
  slide,
  base,
  align,
  accent,
  fg,
  muted,
  maxWidth,
  rule,
}: {
  slide: Slide
  base: number
  align: `center` | `left`
  accent: string
  fg: string
  muted: string
  maxWidth: number
  /** zinc's 2px×40px tick above the eyebrow. */
  rule?: boolean
}) {
  const headlineSize = Math.round(base * 0.082)
  const subSize = Math.round(base * 0.034)
  const eyebrowSize = Math.round(base * 0.026)
  return (
    <div
      data-probe="text"
      style={{
        display: `flex`,
        flexDirection: `column`,
        alignItems: align === `center` ? `center` : `flex-start`,
        textAlign: align,
        maxWidth: `${Math.round(maxWidth)}px`,
        overflow: `hidden`,
      }}
    >
      {rule ? (
        <div
          style={{
            width: `2px`,
            height: `${Math.round(base * 0.055)}px`,
            background: accent,
            marginBottom: `${Math.round(base * 0.026)}px`,
          }}
        />
      ) : null}
      {slide.eyebrow ? (
        <span
          style={{
            fontSize: `${eyebrowSize}px`,
            fontWeight: 600,
            letterSpacing: `0.16em`,
            textTransform: `uppercase`,
            color: accent,
            marginBottom: `${Math.round(base * 0.02)}px`,
          }}
        >
          {slide.eyebrow}
        </span>
      ) : null}
      {slide.headline.map((line) => (
        <span
          key={line}
          data-headline="1"
          style={{
            fontSize: `${headlineSize}px`,
            fontWeight: 700,
            color: fg,
            lineHeight: 1.06,
            letterSpacing: `-0.035em`,
          }}
        >
          {line}
        </span>
      ))}
      {slide.sub ? (
        <span
          style={{
            fontSize: `${subSize}px`,
            fontWeight: 400,
            color: muted,
            lineHeight: 1.35,
            marginTop: `${Math.round(base * 0.028)}px`,
          }}
        >
          {slide.sub}
        </span>
      ) : null}
    </div>
  )
}

function Wordmark({ logo, size, label, fg, muted }: { logo: string; size: number; label?: string; fg: string; muted?: string }) {
  return (
    <div style={{ display: `flex`, alignItems: `center`, gap: `${Math.round(size * 0.34)}px` }}>
      <img src={logo} style={{ width: `${size}px`, height: `${size}px`, borderRadius: `9999px`, display: `block` }} />
      {label ? (
        <span
          style={{
            fontSize: `${Math.round(size * 0.78)}px`,
            fontWeight: 700,
            letterSpacing: `-0.03em`,
            color: muted ?? fg,
          }}
        >
          {label}
        </span>
      ) : null}
    </div>
  )
}

function Canvas({ c, background, children }: { c: Ctx; background: string; children: ReactNode }) {
  return (
    <div
      style={{
        position: `relative`,
        width: `${c.w}px`,
        height: `${c.h}px`,
        overflow: `hidden`,
        background,
        fontFamily: `Geist, system-ui, sans-serif`,
        WebkitFontSmoothing: `antialiased`,
      }}
    >
      {children}
    </div>
  )
}

/* ── Set 1: aurora ───────────────────────────────────────────────────────── */

const AURORA_ACCENTS = [`#A78BFA`, `#5EEAD4`]
const AURORA_BG = [
  `radial-gradient(120% 70% at 78% 8%, rgba(139,92,246,0.28), transparent 60%)`,
  `radial-gradient(110% 65% at 12% 92%, rgba(45,212,191,0.18), transparent 60%)`,
  `#09090B`,
].join(`, `)

function aurora(input: RenderInput) {
  const c = ctx(input)
  const accent = AURORA_ACCENTS[c.slide.index % AURORA_ACCENTS.length]
  const pad = Math.round(c.base * 0.085)
  const screenW = Math.round(c.w * 0.78)
  const frameShadow = `0 0 160px rgba(139,92,246,0.35), 0 40px 90px rgba(0,0,0,0.7)`

  if (c.slide.featureGraphic) {
    return (
      <Canvas c={c} background={AURORA_BG}>
        <div
          style={{
            position: `absolute`,
            inset: 0,
            display: `flex`,
            flexDirection: `column`,
            justifyContent: `center`,
            padding: `0 ${Math.round(c.w * 0.07)}px`,
            gap: `${Math.round(c.h * 0.06)}px`,
          }}
        >
          <Wordmark logo={c.logo} size={Math.round(c.h * 0.17)} label="Exponential" fg="#fafafa" />
          <Headline
            slide={c.slide}
            base={Math.round(c.h * 1.45)}
            align="left"
            accent={accent}
            fg="#fafafa"
            muted="#A1A1AA"
            maxWidth={c.w * 0.82}
          />
        </div>
      </Canvas>
    )
  }

  const hero = !c.slide.shot
  return (
    <Canvas c={c} background={AURORA_BG}>
      <div
        style={{
          position: `absolute`,
          inset: 0,
          display: `flex`,
          flexDirection: `column`,
          alignItems: `center`,
          paddingTop: `${pad}px`,
          gap: `${Math.round(c.base * 0.05)}px`,
        }}
      >
        <Wordmark logo={c.logo} size={Math.round(c.base * 0.075)} fg="#fafafa" />
        <Headline
          slide={c.slide}
          base={c.base}
          align="center"
          accent={accent}
          fg="#fafafa"
          muted="#A1A1AA"
          maxWidth={c.w - pad * 2}
        />
      </div>

      {c.shotUri ? (
        <div
          style={{
            position: `absolute`,
            left: `50%`,
            top: `${Math.round(c.h * (hero ? 0.5 : 0.36))}px`,
            transform: hero ? `translateX(-50%) rotate(-7deg)` : `translateX(-50%)`,
          }}
        >
          <Frame
            screenW={screenW}
            rawWidth={c.rawWidth}
            rawHeight={c.rawHeight}
            src={c.shotUri}
            radius={Math.round(c.w * 0.062)}
            ring={Math.max(4, Math.round(c.w * 0.012))}
            bezel="linear-gradient(160deg, #2a2a30, #0b0b0e)"
            ringColor="rgba(255,255,255,0.16)"
            shadow={frameShadow}
            notch={c.form !== `android-phone`}
          />
        </div>
      ) : null}

      {c.pop && c.shotUri && !hero ? (
        <div style={{ position: `absolute`, left: `${Math.round(c.w * 0.06)}px`, top: `${Math.round(c.h * 0.72)}px` }}>
          <PopOut
            rect={c.pop}
            rawWidth={c.rawWidth}
            rawHeight={c.rawHeight}
            src={c.shotUri}
            targetW={c.w * 0.86}
            rotate={-2.5}
            radius={Math.round(c.base * 0.035)}
            card={{
              background: `#18181B`,
              border: `1px solid rgba(255,255,255,0.14)`,
              boxShadow: `0 44px 90px rgba(0,0,0,0.75), 0 0 90px rgba(139,92,246,0.25)`,
              padding: `6px`,
            }}
          />
        </div>
      ) : null}
    </Canvas>
  )
}

/* ── Set 2: zinc (default) ───────────────────────────────────────────────── */

const ZINC_BG = `linear-gradient(180deg, #09090B 0%, #18181B 100%)`

function zinc(input: RenderInput) {
  const c = ctx(input)
  const pad = Math.round(c.base * 0.085)
  const screenW = Math.round(c.w * 0.86)
  // Alternate which edge the frame bleeds off, slide to slide.
  const rightBleed = c.slide.index % 2 === 0

  if (c.slide.featureGraphic) {
    return (
      <Canvas c={c} background={ZINC_BG}>
        <div
          style={{
            position: `absolute`,
            inset: 0,
            display: `flex`,
            flexDirection: `column`,
            justifyContent: `center`,
            padding: `0 ${Math.round(c.w * 0.07)}px`,
            gap: `${Math.round(c.h * 0.06)}px`,
          }}
        >
          <Wordmark logo={c.logo} size={Math.round(c.h * 0.17)} label="Exponential" fg="#fafafa" />
          <Headline
            slide={c.slide}
            base={Math.round(c.h * 1.45)}
            align="left"
            accent="#71717A"
            fg="#fafafa"
            muted="#A1A1AA"
            maxWidth={c.w * 0.82}
          />
        </div>
      </Canvas>
    )
  }

  // Hero is pure type + wordmark — no device, no capture.
  if (!c.slide.shot) {
    return (
      <Canvas c={c} background={ZINC_BG}>
        <div
          style={{
            position: `absolute`,
            inset: 0,
            display: `flex`,
            flexDirection: `column`,
            justifyContent: `center`,
            padding: `0 ${pad}px`,
            gap: `${Math.round(c.base * 0.09)}px`,
          }}
        >
          <Wordmark logo={c.logo} size={Math.round(c.base * 0.11)} label="Exponential" fg="#fafafa" />
          <Headline
            slide={c.slide}
            base={c.base * 1.25}
            align="left"
            accent="#52525B"
            fg="#fafafa"
            muted="#A1A1AA"
            maxWidth={c.w - pad * 2}
            rule
          />
        </div>
      </Canvas>
    )
  }

  return (
    <Canvas c={c} background={ZINC_BG}>
      <div
        style={{
          position: `absolute`,
          left: `${pad}px`,
          top: `${pad}px`,
          right: `${pad}px`,
        }}
      >
        <Headline
          slide={c.slide}
          base={c.base}
          align="left"
          accent="#52525B"
          fg="#fafafa"
          muted="#A1A1AA"
          maxWidth={c.w - pad * 2}
          rule
        />
      </div>

      {c.shotUri ? (
        <div
          style={{
            position: `absolute`,
            top: `${Math.round(c.h * 0.32)}px`,
            ...(rightBleed
              ? { right: `${Math.round(-c.w * 0.12)}px` }
              : { left: `${Math.round(-c.w * 0.12)}px` }),
          }}
        >
          <Frame
            screenW={screenW}
            rawWidth={c.rawWidth}
            rawHeight={c.rawHeight}
            src={c.shotUri}
            radius={Math.round(c.w * 0.062)}
            ring={Math.max(4, Math.round(c.w * 0.012))}
            bezel="#0B0B0E"
            ringColor="rgba(255,255,255,0.10)"
            shadow="0 60px 120px rgba(0,0,0,0.55)"
            notch={c.form !== `android-phone`}
          />
        </div>
      ) : null}

      {c.pop && c.shotUri ? (
        <div
          style={{
            position: `absolute`,
            top: `${Math.round(c.h * 0.74)}px`,
            ...(rightBleed ? { left: `${Math.round(c.w * 0.05)}px` } : { right: `${Math.round(c.w * 0.05)}px` }),
          }}
        >
          <PopOut
            rect={c.pop}
            rawWidth={c.rawWidth}
            rawHeight={c.rawHeight}
            src={c.shotUri}
            targetW={c.w * 0.8}
            rotate={0}
            radius={Math.round(c.base * 0.032)}
            card={{
              background: `rgba(255,255,255,0.06)`,
              border: `1px solid rgba(255,255,255,0.12)`,
              backdropFilter: `blur(24px)`,
              WebkitBackdropFilter: `blur(24px)`,
              boxShadow: `0 40px 80px rgba(0,0,0,0.6)`,
              padding: `${Math.round(c.base * 0.018)}px`,
            }}
          />
        </div>
      ) : null}
    </Canvas>
  )
}

/* ── Set 3: colorblock ───────────────────────────────────────────────────── */

const BLOCK_ACCENTS = [`#4F46E5`, `#0EA5E9`, `#10B981`, `#F59E0B`, `#F43F5E`, `#8B5CF6`, `#64748B`]

function colorblock(input: RenderInput) {
  const c = ctx(input)
  const accent = BLOCK_ACCENTS[c.slide.index % BLOCK_ACCENTS.length]
  const pad = Math.round(c.base * 0.085)
  const screenW = Math.round(c.w * 0.72)

  if (c.slide.featureGraphic) {
    return (
      <Canvas c={c} background={`linear-gradient(120deg, ${accent} 0%, #0EA5E9 60%, #8B5CF6 100%)`}>
        <div
          style={{
            position: `absolute`,
            inset: 0,
            display: `flex`,
            flexDirection: `column`,
            justifyContent: `center`,
            padding: `0 ${Math.round(c.w * 0.07)}px`,
            gap: `${Math.round(c.h * 0.05)}px`,
          }}
        >
          <Wordmark logo={c.logo} size={Math.round(c.h * 0.2)} label="Exponential" fg="#ffffff" />
          <Headline
            slide={c.slide}
            base={Math.round(c.h * 1.3)}
            align="left"
            accent="rgba(255,255,255,0.75)"
            fg="#ffffff"
            muted="rgba(255,255,255,0.85)"
            maxWidth={c.w * 0.82}
          />
        </div>
      </Canvas>
    )
  }

  const hero = !c.slide.shot
  return (
    <Canvas c={c} background="#09090B">
      {/* Oversized rotated accent slab behind the type. */}
      <div
        style={{
          position: `absolute`,
          left: `${Math.round(-c.w * 0.18)}px`,
          top: `${Math.round(-c.h * 0.16)}px`,
          width: `${Math.round(c.w * 1.4)}px`,
          height: `${Math.round(c.h * (hero ? 0.78 : 0.56))}px`,
          background: accent,
          transform: `rotate(-4deg)`,
        }}
      />
      <div
        style={{
          position: `absolute`,
          left: `${pad}px`,
          right: `${pad}px`,
          top: `${Math.round(c.h * (hero ? 0.2 : 0.07))}px`,
          display: `flex`,
          flexDirection: `column`,
          gap: `${Math.round(c.base * 0.05)}px`,
        }}
      >
        <Wordmark logo={c.logo} size={Math.round(c.base * 0.085)} label={hero ? `Exponential` : undefined} fg="#ffffff" />
        <Headline
          slide={c.slide}
          base={hero ? c.base * 1.2 : c.base}
          align="left"
          accent="rgba(255,255,255,0.78)"
          fg="#ffffff"
          muted="rgba(255,255,255,0.88)"
          maxWidth={c.w - pad * 2}
        />
      </div>

      {c.shotUri ? (
        <div
          style={{
            position: `absolute`,
            left: `50%`,
            top: `${Math.round(c.h * (hero ? 0.62 : 0.46))}px`,
            transform: hero ? `translateX(-46%) rotate(6deg)` : `translateX(-50%)`,
          }}
        >
          <Frame
            screenW={screenW}
            rawWidth={c.rawWidth}
            rawHeight={c.rawHeight}
            src={c.shotUri}
            radius={Math.round(c.w * 0.06)}
            ring={Math.max(5, Math.round(c.w * 0.016))}
            bezel="#F4F4F5"
            ringColor="rgba(0,0,0,0.12)"
            shadow="0 50px 100px rgba(0,0,0,0.6)"
            notch={c.form !== `android-phone`}
          />
        </div>
      ) : null}

      {c.pop && c.shotUri && !hero ? (
        <div style={{ position: `absolute`, right: `${Math.round(c.w * 0.04)}px`, top: `${Math.round(c.h * 0.75)}px` }}>
          <PopOut
            rect={c.pop}
            rawWidth={c.rawWidth}
            rawHeight={c.rawHeight}
            src={c.shotUri}
            targetW={c.w * 0.8}
            rotate={3}
            radius={Math.round(c.base * 0.03)}
            card={{
              background: `#FFFFFF`,
              padding: `${Math.round(c.base * 0.016)}px`,
              boxShadow: `0 40px 90px ${accent}66, 0 20px 40px rgba(0,0,0,0.45)`,
            }}
          />
        </div>
      ) : null}
    </Canvas>
  )
}

export const STYLE_SETS: Record<StyleSetId, (input: RenderInput) => ReactNode> = {
  aurora,
  zinc,
  colorblock,
}
