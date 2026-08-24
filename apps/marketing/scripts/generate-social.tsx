/* Dev-only social ad-material generator (EXP-597). Produces the committed
   PNGs in public/social/:

     x-header.png       — 1500×500 X/Twitter profile header (pure branding)
     card-<feature>.png — 1600×900 share cards: headline + a real web-app
                          screenshot in a browser bezel on the brand gradient,
                          the landscape sibling of the store panels
                          (scripts/store/)

   The cards compose the RAW captures in apps/marketing/social-shots/ (not
   committed, EXP-348) produced by `bun run screenshots:social` in apps/web —
   see capture-social-shots.ts there for the full capture recipe. Rerun this
   after recapturing:

     bun run scripts/generate-social.tsx

   Same satori + resvg recipe as generate-og.tsx; fonts in scripts/assets/. */

import satori from "satori"
import { Resvg } from "@resvg/resvg-js"
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const HERE = dirname(fileURLToPath(import.meta.url))
const ROOT = resolve(HERE, `..`)
const ASSETS = resolve(HERE, `assets`)
const SHOTS = resolve(ROOT, `social-shots`)
const OUT = resolve(ROOT, `public/social`)

const FG = `#fafafa`
const MUTED = `#a1a1aa`
const BG_TOP = `#09090b`
const BG_BOTTOM = `#18181b`

const fonts = [
  { name: `Geist`, data: readFileSync(resolve(ASSETS, `Geist-Regular.ttf`)), weight: 400 as const, style: `normal` as const },
  { name: `Geist`, data: readFileSync(resolve(ASSETS, `Geist-SemiBold.ttf`)), weight: 600 as const, style: `normal` as const },
  { name: `Geist`, data: readFileSync(resolve(ASSETS, `Geist-Bold.ttf`)), weight: 700 as const, style: `normal` as const },
]

function markDataUri(size: number, variant: `light` | `dark` = `light`): string {
  const svg = readFileSync(resolve(ROOT, `public/logo-${variant}.svg`), `utf8`)
  const png = new Resvg(svg, { fitTo: { mode: `width`, value: size } }).render().asPng()
  return `data:image/png;base64,${png.toString(`base64`)}`
}

async function render(node: React.ReactNode, width: number, height: number): Promise<Buffer> {
  const svg = await satori(node, { width, height, fonts })
  return Buffer.from(new Resvg(svg, { fitTo: { mode: `width`, value: width } }).render().asPng())
}

/* ── X profile header ─────────────────────────────────────────────────────
   1500×500 is the canonical upload size. X crops aggressively per device and
   the avatar circle overlays the bottom-left, so everything lives in the
   central band and nothing sits in the corners. */

const HEADER_W = 1500
const HEADER_H = 500

function Header({ mark, watermark }: { mark: string; watermark: string }) {
  return (
    <div
      style={{
        width: `${HEADER_W}px`,
        height: `${HEADER_H}px`,
        display: `flex`,
        flexDirection: `column`,
        alignItems: `center`,
        justifyContent: `center`,
        backgroundImage: `linear-gradient(180deg, ${BG_TOP} 0%, ${BG_BOTTOM} 100%)`,
        fontFamily: `Geist`,
        position: `relative`,
        overflow: `hidden`,
      }}
    >
      {/* Oversized faint mark bleeding off the right edge. */}
      <img
        src={watermark}
        width={620}
        height={620}
        style={{ position: `absolute`, right: `-160px`, top: `-60px`, opacity: 0.07 }}
      />
      <div style={{ display: `flex`, alignItems: `center`, gap: `28px` }}>
        <img src={mark} width={84} height={84} style={{ borderRadius: `9999px` }} />
        <span style={{ fontSize: `76px`, fontWeight: 700, color: FG, letterSpacing: `-0.03em` }}>
          The next generation dev platform
        </span>
      </div>
      <span style={{ fontSize: `33px`, fontWeight: 400, color: MUTED, marginTop: `26px` }}>
        Issues, customer feedback and coding agents in one realtime tracker.
      </span>
      <span style={{ fontSize: `26px`, fontWeight: 600, color: `#71717a`, marginTop: `30px`, letterSpacing: `0.02em` }}>
        exponential.at
      </span>
    </div>
  )
}

/* ── Feature share cards ──────────────────────────────────────────────────
   1600×900: headline block up top, the raw capture in a browser-style bezel
   below, running off the bottom edge like the store panels. */

const CARD_W = 1600
const CARD_H = 900

/** `shift` scrolls the capture inside the bezel — the fraction of the shot's
 *  height hidden above the fold, for pages whose story lives further down. */
type Card = { shot: string; file: string; headline: string; sub: string; shift?: number }

const CARDS: Card[] = [
  {
    shot: `board`,
    file: `card-board.png`,
    headline: `The next-gen dev platform`,
    sub: `Issues, customer feedback and coding agents in one realtime tracker`,
  },
  {
    shot: `issue`,
    file: `card-issues.png`,
    headline: `Everything on the issue`,
    sub: `Markdown, checklists and mentions — synced live to every device`,
  },
  {
    shot: `steering`,
    file: `card-steering.png`,
    headline: `Local agents. Your machines.`,
    sub: `Watch a coding run live and steer it from anywhere`,
    shift: 0.22,
  },
  {
    shot: `review-diff`,
    file: `card-reviews.png`,
    headline: `Review and merge`,
    sub: `Every open PR across your boards — one click to squash-merge`,
  },
  {
    shot: `actions`,
    file: `card-actions.png`,
    headline: `AI actions. Automate anything.`,
    sub: `Saved agent playbooks your whole team can run`,
  },
  {
    shot: `inbox`,
    file: `card-inbox.png`,
    headline: `A calm inbox`,
    sub: `Assignments, mentions and merged PRs in one place`,
  },
  {
    shot: `support`,
    file: `card-support.png`,
    headline: `Support built in`,
    sub: `Customer tickets land next to the code — escalate to an issue in a click`,
  },
]

function CardPanel({ copy, shot, mark }: { copy: Card; shot: string; mark: string }) {
  const shotWidth = 1376
  // Raw captures are a 1600×1000 viewport at 2x.
  const shotHeight = Math.round((shotWidth / 3200) * 2000)
  return (
    <div
      style={{
        width: `${CARD_W}px`,
        height: `${CARD_H}px`,
        display: `flex`,
        flexDirection: `column`,
        alignItems: `center`,
        overflow: `hidden`,
        backgroundImage: `linear-gradient(180deg, ${BG_TOP} 0%, ${BG_BOTTOM} 100%)`,
        fontFamily: `Geist`,
      }}
    >
      <div
        style={{
          display: `flex`,
          alignItems: `center`,
          gap: `20px`,
          padding: `56px 80px 14px`,
        }}
      >
        <img src={mark} width={56} height={56} style={{ borderRadius: `9999px` }} />
        <span style={{ fontSize: `58px`, fontWeight: 700, color: FG, letterSpacing: `-0.03em` }}>
          {copy.headline}
        </span>
      </div>
      <span style={{ fontSize: `26px`, fontWeight: 400, color: MUTED, marginBottom: `40px` }}>{copy.sub}</span>

      {/* Browser bezel: traffic lights + the capture, off the bottom edge. */}
      <div
        style={{
          display: `flex`,
          flexDirection: `column`,
          borderRadius: `18px`,
          backgroundColor: `#050505`,
          border: `1px solid rgba(255,255,255,0.14)`,
          overflow: `hidden`,
        }}
      >
        <div style={{ display: `flex`, gap: `9px`, padding: `14px 18px` }}>
          <div style={{ display: `flex`, width: `13px`, height: `13px`, borderRadius: `9999px`, backgroundColor: `#ff5f57` }} />
          <div style={{ display: `flex`, width: `13px`, height: `13px`, borderRadius: `9999px`, backgroundColor: `#febc2e` }} />
          <div style={{ display: `flex`, width: `13px`, height: `13px`, borderRadius: `9999px`, backgroundColor: `#28c840` }} />
        </div>
        <div
          style={{
            display: `flex`,
            width: `${shotWidth}px`,
            height: `${shotHeight - Math.round(shotHeight * (copy.shift ?? 0))}px`,
            overflow: `hidden`,
          }}
        >
          <img
            src={shot}
            width={shotWidth}
            height={shotHeight}
            style={{ marginTop: `-${Math.round(shotHeight * (copy.shift ?? 0))}px` }}
          />
        </div>
      </div>
    </div>
  )
}

async function main() {
  mkdirSync(OUT, { recursive: true })
  const mark = markDataUri(256)
  const watermark = markDataUri(1240)

  writeFileSync(resolve(OUT, `x-header.png`), await render(<Header mark={mark} watermark={watermark} />, HEADER_W, HEADER_H))
  console.log(`x-header.png ${HEADER_W}×${HEADER_H}`)

  let missing = 0
  for (const card of CARDS) {
    const raw = resolve(SHOTS, `${card.shot}.png`)
    if (!existsSync(raw)) {
      console.warn(`skip ${card.file} — no capture at social-shots/${card.shot}.png (run screenshots:social in apps/web)`)
      missing++
      continue
    }
    const shot = `data:image/png;base64,${readFileSync(raw).toString(`base64`)}`
    writeFileSync(resolve(OUT, card.file), await render(<CardPanel copy={card} shot={shot} mark={mark} />, CARD_W, CARD_H))
    console.log(`${card.file} ${CARD_W}×${CARD_H}`)
  }
  if (missing > 0) console.warn(`${missing} card(s) skipped — capture first`)
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
