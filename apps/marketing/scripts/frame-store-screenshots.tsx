/* Dev-only store-screenshot framer (EXP-580). Turns the three headline shots
   of the App Store / Play Store set (board, steering, actions) into short
   marketing panels — a headline + the raw screenshot in a device bezel on the
   brand gradient — and leaves the other five shots untouched. Runs AFTER the
   fastlane captures and BEFORE `fastlane sync_store`:

     bun run screenshots:frame                  # from the repo root, both platforms
     bun run screenshots:frame --platform ios   # or android
     bun run screenshots:frame --dry-run        # write to out/ instead of in place
     bun run screenshots:frame --check          # exit 1 on any size mismatch

   It rewrites the capture outputs IN PLACE at exactly the raw pixel size (App
   Store Connect rejects off-size 6.9"/13" images) and keeps the untouched
   originals in a sibling `*-raw/` dir, which is also the source on re-runs, so
   the step is idempotent. Nothing here is committed: store screenshots stay out
   of git (EXP-348). Same satori + resvg recipe as generate-og.tsx. */

import satori from "satori"
import { Resvg } from "@resvg/resvg-js"
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs"
import { basename, dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const HERE = dirname(fileURLToPath(import.meta.url))
const REPO = resolve(HERE, `../../..`)
const ASSETS = resolve(HERE, `assets`)

const FG = `#fafafa`
const MUTED = `#a1a1aa`
const BG_TOP = `#09090b`
const BG_BOTTOM = `#18181b`

type Copy = { headline: string[]; sub?: string }

/* Keyed by the shot name shared by both UI tests (`01_board` / `1_board`). Keep
   the copy SHORT: positioning, not feature walkthroughs. */
const FRAMED: Record<string, Copy> = {
  board: { headline: [`The next-gen`, `dev platform`], sub: `Built for dev teams` },
  steering: { headline: [`Local agents.`, `Your machines.`], sub: `Steer them from anywhere` },
  actions: { headline: [`AI actions.`, `Automate anything.`] },
}

type Platform = `ios` | `android`

const INPUTS: Record<Platform, { dir: string; raw: string }> = {
  ios: {
    dir: resolve(REPO, `apps/ios/fastlane/screenshots/en-US`),
    raw: resolve(REPO, `apps/ios/fastlane/screenshots-raw/en-US`),
  },
  android: {
    dir: resolve(REPO, `apps/android/fastlane/metadata/android/en-US/images/phoneScreenshots`),
    raw: resolve(REPO, `apps/android/fastlane/metadata/android/en-US/images/phoneScreenshots-raw`),
  },
}

const fonts = [
  { name: `Geist`, data: readFileSync(resolve(ASSETS, `Geist-Regular.ttf`)), weight: 400 as const, style: `normal` as const },
  { name: `Geist`, data: readFileSync(resolve(ASSETS, `Geist-SemiBold.ttf`)), weight: 600 as const, style: `normal` as const },
  { name: `Geist`, data: readFileSync(resolve(ASSETS, `Geist-Bold.ttf`)), weight: 700 as const, style: `normal` as const },
]

function markDataUri(size: number): string {
  const svg = readFileSync(resolve(REPO, `apps/marketing/public/logo-light.svg`), `utf8`)
  const png = new Resvg(svg, { fitTo: { mode: `width`, value: size } }).render().asPng()
  return `data:image/png;base64,${png.toString(`base64`)}`
}

/** Width/height from the PNG IHDR chunk — no image dep needed. */
function pngSize(buf: Buffer): { width: number; height: number } {
  if (buf.readUInt32BE(12) !== 0x49484452) throw new Error(`not a PNG (no IHDR)`)
  return { width: buf.readUInt32BE(16), height: buf.readUInt32BE(20) }
}

/* Framed outputs carry this PNG tEXt marker so a re-run can tell "already
   framed, use the stashed raw" from "fresh capture, stash it first" by the
   file itself — an older run trusted whatever sat in `*-raw/`, and a stale
   stash from a dry run silently replaced three real captures. */
const MARKER_KEY = `exp-store-frame`

function hasMarker(buf: Buffer): boolean {
  return buf.includes(Buffer.from(`${MARKER_KEY}\0`))
}

/** Insert a tEXt chunk right after IHDR. */
function withMarker(png: Buffer): Buffer {
  const data = Buffer.from(`${MARKER_KEY}\0EXP-580`, `latin1`)
  const type = Buffer.from(`tEXt`, `latin1`)
  const len = Buffer.alloc(4)
  len.writeUInt32BE(data.length)
  const crc = Buffer.alloc(4)
  crc.writeUInt32BE(Bun.hash.crc32(Buffer.concat([type, data])) >>> 0)
  const ihdrEnd = 8 + 4 + 4 + 13 + 4
  return Buffer.concat([png.subarray(0, ihdrEnd), len, type, data, crc, png.subarray(ihdrEnd)])
}

/** `iPhone 17 Pro Max-04_steering.png` / `4_steering.png` → `steering`. */
function shotKey(file: string): string | null {
  const m = /(?:^|-)\d+_([a-z-]+)\.png$/.exec(basename(file))
  return m ? m[1] : null
}

function Panel({ copy, shot, mark, width, height }: { copy: Copy; shot: string; mark: string; width: number; height: number }) {
  // Type scales from the SHORTER reference so the squat iPad canvas doesn't get
  // phone-proportioned headlines that eat half the screenshot.
  const base = Math.min(width, Math.round(height / 2))
  const pad = Math.round(base * 0.075)
  const headlineSize = Math.round(base * 0.082)
  const subSize = Math.round(base * 0.036)
  const markSize = Math.round(base * 0.065)
  const shotWidth = Math.round(width * 0.84)
  const shotHeight = Math.round((shotWidth / width) * height)
  const radius = Math.round(width * 0.065)
  const ring = Math.max(6, Math.round(width * 0.011))
  return (
    <div
      style={{
        width: `${width}px`,
        height: `${height}px`,
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
          flexDirection: `column`,
          alignItems: `center`,
          width: `100%`,
          padding: `${pad}px ${pad}px ${Math.round(pad * 0.7)}px`,
          gap: `${Math.round(base * 0.03)}px`,
        }}
      >
        <img src={mark} width={markSize} height={markSize} style={{ borderRadius: `9999px` }} />
        <div style={{ display: `flex`, flexDirection: `column`, alignItems: `center` }}>
          {copy.headline.map((line) => (
            <span
              key={line}
              style={{
                fontSize: `${headlineSize}px`,
                fontWeight: 700,
                color: FG,
                lineHeight: 1.08,
                letterSpacing: `-0.03em`,
                textAlign: `center`,
              }}
            >
              {line}
            </span>
          ))}
        </div>
        {copy.sub ? (
          <span style={{ fontSize: `${subSize}px`, fontWeight: 400, color: MUTED, lineHeight: 1.3, textAlign: `center` }}>
            {copy.sub}
          </span>
        ) : null}
      </div>

      {/* Device bezel; the screenshot runs off the bottom edge. */}
      <div
        style={{
          display: `flex`,
          padding: `${ring}px`,
          borderRadius: `${radius}px`,
          backgroundColor: `#050505`,
          border: `1px solid rgba(255,255,255,0.14)`,
          // (no boxShadow: resvg panics on satori's shadow filter at these sizes)
        }}
      >
        <div
          style={{
            display: `flex`,
            width: `${shotWidth}px`,
            height: `${shotHeight}px`,
            borderRadius: `${radius - ring}px`,
            overflow: `hidden`,
          }}
        >
          <img src={shot} width={shotWidth} height={shotHeight} />
        </div>
      </div>
    </div>
  )
}

async function frame(rawPng: Buffer, copy: Copy, mark: string): Promise<Buffer> {
  const { width, height } = pngSize(rawPng)
  const shot = `data:image/png;base64,${rawPng.toString(`base64`)}`
  const svg = await satori(<Panel copy={copy} shot={shot} mark={mark} width={width} height={height} />, {
    width,
    height,
    fonts,
  })
  return Buffer.from(new Resvg(svg, { fitTo: { mode: `width`, value: width } }).render().asPng())
}

async function main() {
  const argv = process.argv.slice(2)
  const platformArg = argv[argv.indexOf(`--platform`) + 1]
  const platforms: Platform[] =
    argv.includes(`--platform`) && platformArg !== `all` ? [platformArg as Platform] : [`ios`, `android`]
  const dryRun = argv.includes(`--dry-run`)
  const check = argv.includes(`--check`)
  for (const p of platforms) if (!INPUTS[p]) throw new Error(`unknown platform ${p}`)

  const mark = markDataUri(256)
  let framed = 0
  let mismatches = 0
  for (const platform of platforms) {
    const { dir, raw } = INPUTS[platform]
    if (!existsSync(dir)) {
      console.warn(`[${platform}] no captures at ${dir} — run \`fastlane screenshots\` first`)
      continue
    }
    const outDir = dryRun ? resolve(HERE, `../out/store-frames`, platform) : dir
    mkdirSync(outDir, { recursive: true })
    mkdirSync(raw, { recursive: true })
    for (const file of readdirSync(dir).filter((f) => f.endsWith(`.png`)).sort()) {
      const key = shotKey(file)
      const copy = key ? FRAMED[key] : undefined
      if (!copy) continue
      const rawPath = join(raw, file)
      const current = readFileSync(join(dir, file))
      // A fresh capture (no marker) is the truth: stash it, overwriting any
      // older raw. An already-framed file reads its stashed raw back instead so
      // framing never compounds.
      if (!hasMarker(current)) copyFileSync(join(dir, file), rawPath)
      else if (!existsSync(rawPath)) throw new Error(`${file} is already framed and its raw copy is missing — recapture it`)
      const rawPng = readFileSync(rawPath)
      const out = withMarker(await frame(rawPng, copy, mark))
      const a = pngSize(rawPng)
      const b = pngSize(out)
      const same = a.width === b.width && a.height === b.height
      if (!same) mismatches++
      writeFileSync(join(outDir, file), out)
      framed++
      console.log(`[${platform}] ${file} ${b.width}×${b.height}${same ? `` : ` — MISMATCH, raw is ${a.width}×${a.height}`}`)
    }
  }
  console.log(`framed ${framed} shot(s)${dryRun ? ` (dry run → apps/marketing/out/store-frames)` : ``}`)
  if (check && mismatches > 0) {
    console.error(`${mismatches} framed image(s) differ from their raw size`)
    process.exit(1)
  }
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
