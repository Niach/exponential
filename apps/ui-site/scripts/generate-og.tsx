/* Dev-only OG image generator for ui.exponential.at, modelled on
   apps/marketing/scripts/generate-og.tsx. Produces the committed 1200×630
   public/og/og-ui.png every page of the manifest (src/lib/routes.ts) uses.
   NOT part of the build: run it by hand when the pitch changes:

     bun run scripts/generate-og.tsx

   satori renders JSX → SVG, @resvg/resvg-js rasterizes it. The Geist files
   and the brand mark are exponential.at's (apps/marketing), never copied. */
import satori from "satori"
import { Resvg } from "@resvg/resvg-js"
import { mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const HERE = dirname(fileURLToPath(import.meta.url))
const ROOT = resolve(HERE, `..`)
const MARKETING = resolve(ROOT, `../marketing`)
const FONTS = resolve(MARKETING, `scripts/assets`)
const OUT = resolve(ROOT, `public/og`)

const BG = `#09090b`
const FG = `#fafafa`
const MUTED = `#a1a1aa`
const DIM = `#71717a`
const BORDER = `#27272a`
const CHIP = `#18181b`

const fonts = [
  { name: `Geist`, data: readFileSync(resolve(FONTS, `Geist-Regular.ttf`)), weight: 400 as const, style: `normal` as const },
  { name: `Geist`, data: readFileSync(resolve(FONTS, `Geist-SemiBold.ttf`)), weight: 600 as const, style: `normal` as const },
  { name: `Geist`, data: readFileSync(resolve(FONTS, `Geist-Bold.ttf`)), weight: 700 as const, style: `normal` as const },
]

/* The Exponential mark, rasterized once and embedded as a data URI (satori
   renders bitmaps reliably, arbitrary inline SVG less so). */
function markDataUri(size: number): string {
  const svg = readFileSync(resolve(MARKETING, `public/logo-light.svg`), `utf8`)
  const png = new Resvg(svg, { fitTo: { mode: `width`, value: size } }).render().asPng()
  return `data:image/png;base64,${png.toString(`base64`)}`
}

const RENDERERS = [`React`, `SwiftUI`, `Jetpack Compose`, `gpui`]

function Card({ mark }: { mark: string }) {
  return (
    <div
      style={{
        width: `1200px`,
        height: `630px`,
        display: `flex`,
        flexDirection: `column`,
        justifyContent: `space-between`,
        backgroundColor: BG,
        backgroundImage: `linear-gradient(to bottom, #050507, #111114)`,
        padding: `72px`,
        fontFamily: `Geist`,
      }}
    >
      <div style={{ display: `flex`, alignItems: `center`, gap: `20px` }}>
        <img src={mark} width={64} height={64} style={{ borderRadius: `9999px` }} />
        <span style={{ fontSize: `36px`, fontWeight: 600, color: FG, letterSpacing: `-0.02em` }}>Exponential UI</span>
        <span
          style={{
            marginLeft: `8px`,
            fontSize: `20px`,
            color: MUTED,
            border: `1px solid ${BORDER}`,
            borderRadius: `9999px`,
            padding: `6px 16px`,
          }}
        >
          A2UI v0.9 · Apache-2.0
        </span>
      </div>

      <div style={{ display: `flex`, flexDirection: `column`, gap: `24px` }}>
        <div style={{ display: `flex`, flexDirection: `column`, fontSize: `80px`, fontWeight: 700, lineHeight: 1.04, letterSpacing: `-0.035em` }}>
          <span style={{ color: FG }}>Generative UI,</span>
          <span style={{ color: MUTED }}>native on every platform.</span>
        </div>
        <span style={{ fontSize: `30px`, fontWeight: 400, color: MUTED, lineHeight: 1.35, maxWidth: `980px` }}>
          An agent sends a surface as A2UI. Open renderers paint it natively on the web, iOS, Android and the desktop.
        </span>
      </div>

      <div
        style={{
          display: `flex`,
          alignItems: `center`,
          justifyContent: `space-between`,
          borderTop: `1px solid ${BORDER}`,
          paddingTop: `28px`,
        }}
      >
        <div style={{ display: `flex`, gap: `12px` }}>
          {RENDERERS.map((r) => (
            <span
              key={r}
              style={{
                fontSize: `22px`,
                fontWeight: 600,
                color: FG,
                backgroundColor: CHIP,
                border: `1px solid ${BORDER}`,
                borderRadius: `10px`,
                padding: `8px 16px`,
              }}
            >
              {r}
            </span>
          ))}
        </div>
        <span style={{ fontSize: `26px`, fontWeight: 500, color: DIM }}>ui.exponential.at</span>
      </div>
    </div>
  )
}

mkdirSync(OUT, { recursive: true })
const svg = await satori(<Card mark={markDataUri(128)} />, { width: 1200, height: 630, fonts })
const png = new Resvg(svg, { fitTo: { mode: `width`, value: 1200 } }).render().asPng()
writeFileSync(resolve(OUT, `og-ui.png`), png)
console.log(`wrote public/og/og-ui.png (${png.length} bytes)`)
