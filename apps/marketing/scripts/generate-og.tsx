/* Dev-only OG image generator. Produces the committed 1200×630 PNGs in
   public/og/ used by the SEO block (src/lib/seo.ts). NOT part of the build —
   run it by hand when the branding or headlines change:

     bun run scripts/generate-og.tsx

   satori renders JSX → SVG (dark-branded card with the Exponential mark), and
   @resvg/resvg-js rasterizes SVG → PNG. Fonts are bundled in scripts/assets/. */

import satori from "satori"
import { Resvg } from "@resvg/resvg-js"
import { readFileSync, writeFileSync, mkdirSync } from "node:fs"
import { resolve, dirname } from "node:path"
import { fileURLToPath } from "node:url"

const HERE = dirname(fileURLToPath(import.meta.url))
const ROOT = resolve(HERE, `..`)
const ASSETS = resolve(HERE, `assets`)
const OUT = resolve(ROOT, `public/og`)

const BG = `#09090b`
const FG = `#fafafa`
const MUTED = `#a1a1aa`
const BORDER = `#27272a`

const fonts = [
  { name: `Geist`, data: readFileSync(resolve(ASSETS, `Geist-Regular.ttf`)), weight: 400 as const, style: `normal` as const },
  { name: `Geist`, data: readFileSync(resolve(ASSETS, `Geist-SemiBold.ttf`)), weight: 600 as const, style: `normal` as const },
  { name: `Geist`, data: readFileSync(resolve(ASSETS, `Geist-Bold.ttf`)), weight: 700 as const, style: `normal` as const },
]

/* The circular Exponential mark (white variant), rasterized once and embedded
   as a data URI — satori renders bitmaps reliably, arbitrary inline SVG less so. */
function markDataUri(size: number): string {
  const svg = readFileSync(resolve(ROOT, `public/logo-light.svg`), `utf8`)
  const png = new Resvg(svg, {
    fitTo: { mode: `width`, value: size },
  })
    .render()
    .asPng()
  return `data:image/png;base64,${png.toString(`base64`)}`
}

type Card = { file: string; title: string; subtitle: string }

const CARDS: Card[] = [
  {
    file: `og-home.png`,
    title: `Issue tracking that ships code.`,
    subtitle: `An issue tracker with a built-in coding IDE. Feedback in, pull requests out.`,
  },
  {
    file: `og-pricing.png`,
    title: `Free for individuals. Affordable for teams.`,
    subtitle: `Per-seat pricing. Local AI agents free on every tier. Self-host free and unlimited.`,
  },
  {
    file: `og-download.png`,
    title: `Native on every platform.`,
    subtitle: `The Rust desktop IDE for macOS and Linux, plus iOS and Android companions.`,
  },
  {
    file: `og-docs.png`,
    title: `Documentation`,
    subtitle: `Issues, the desktop and mobile apps, local AI agents, and integrations.`,
  },
  {
    file: `og-default.png`,
    title: `Exponential`,
    subtitle: `Issue tracking with a built-in coding IDE and local AI agents.`,
  },
]

function CardEl({ card, mark }: { card: Card; mark: string }) {
  return (
    <div
      style={{
        width: `1200px`,
        height: `630px`,
        display: `flex`,
        flexDirection: `column`,
        justifyContent: `space-between`,
        backgroundColor: BG,
        padding: `72px`,
        fontFamily: `Geist`,
      }}
    >
      {/* Brand row */}
      <div style={{ display: `flex`, alignItems: `center`, gap: `20px` }}>
        <img src={mark} width={64} height={64} style={{ borderRadius: `9999px` }} />
        <span style={{ fontSize: `36px`, fontWeight: 600, color: FG, letterSpacing: `-0.02em` }}>
          Exponential
        </span>
      </div>

      {/* Headline */}
      <div style={{ display: `flex`, flexDirection: `column`, gap: `24px` }}>
        <span
          style={{
            fontSize: `76px`,
            fontWeight: 700,
            color: FG,
            lineHeight: 1.05,
            letterSpacing: `-0.03em`,
            maxWidth: `1000px`,
          }}
        >
          {card.title}
        </span>
        <span
          style={{
            fontSize: `32px`,
            fontWeight: 400,
            color: MUTED,
            lineHeight: 1.3,
            maxWidth: `940px`,
          }}
        >
          {card.subtitle}
        </span>
      </div>

      {/* Footer */}
      <div
        style={{
          display: `flex`,
          alignItems: `center`,
          borderTop: `1px solid ${BORDER}`,
          paddingTop: `28px`,
        }}
      >
        <span style={{ fontSize: `28px`, fontWeight: 500, color: MUTED }}>exponential.at</span>
      </div>
    </div>
  )
}

async function main() {
  mkdirSync(OUT, { recursive: true })
  const mark = markDataUri(128)
  for (const card of CARDS) {
    const svg = await satori(<CardEl card={card} mark={mark} />, {
      width: 1200,
      height: 630,
      fonts,
    })
    const png = new Resvg(svg, { fitTo: { mode: `width`, value: 1200 } })
      .render()
      .asPng()
    writeFileSync(resolve(OUT, card.file), png)
    console.log(`wrote public/og/${card.file} (${png.length} bytes)`)
  }
  console.log(`OG images complete`)
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
