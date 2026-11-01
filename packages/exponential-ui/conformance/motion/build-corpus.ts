// EXP-1602: assembles the eui-motion conformance corpus (corpus/*.json +
// corpus/images/) from upstream checkouts and writes corpus/SOURCES.md.
//
//   bun run conformance/motion/build-corpus.ts <lottie-spec> <lottie-web> <skia>
//
// <lottie-spec> = a checkout of github.com/lottie/lottie-spec, <lottie-web> =
// github.com/airbnb/lottie-web, <skia> = skia.googlesource.com/skia (sparse:
// resources/skottie). Files carrying expressions (`"x"` strings on animated
// properties) are STRIPPED of them (the engine refuses expressions by design;
// the references are rendered from the same stripped file). Files > 2 MB are
// skipped. Rerun render-refs.ts afterwards.

import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs"
import { execFileSync } from "node:child_process"
import { join } from "node:path"

const [specRoot, webRoot, skiaRoot] = process.argv.slice(2)
if (!specRoot || !webRoot || !skiaRoot) throw new Error(`usage: build-corpus.ts <lottie-spec> <lottie-web> <skia>`)

type Source = `spec` | `web` | `skia`
interface Pick {
  source: Source
  /** Path inside the source checkout. */
  path: string
  /** The corpus name (without `.json`). */
  name: string
  /** What the file exercises. */
  covers: string
  /** Text layers: `glyphs` (ships `chars`: comparable), `fonts` (system fonts: not comparable). */
  text?: `glyphs` | `fonts`
}

const spec = (file: string, covers: string): Pick => ({ source: `spec`, path: `docs/static/examples/${file}.json`, name: `spec-${file.replace(/_/g, `-`)}`, covers })
const skia = (file: string, covers: string, text?: Pick[`text`]): Pick => ({ source: `skia`, path: `resources/skottie/${file}.json`, name: file.replace(/^skottie[-_]/, `skottie-`).replace(/_/g, `-`), covers, ...(text ? { text } : {}) })
const web = (file: string, covers: string, text?: Pick[`text`]): Pick => ({ source: `web`, path: `test/animations/${file}.json`, name: `web-${file.replace(/[/_]/g, `-`)}`, covers, ...(text ? { text } : {}) })

const PICKS: Pick[] = [
  spec(`blend-mode`, `blend modes, star, ellipse, rect`),
  spec(`ellipse`, `ellipse, stroke`),
  spec(`fill`, `path, fill opacity`),
  spec(`gradient`, `linear + radial gradient fills, animated stops`),
  spec(`gradient-stroke`, `gradient strokes, dashes`),
  spec(`mask`, `additive mask, dashes`),
  spec(`masks`, `mask on a solid layer, dashes`),
  spec(`matte`, `alpha matte`),
  spec(`metadata`, `rect, stroke`),
  spec(`path`, `animated path`),
  spec(`pucker_bloat`, `pucker/bloat modifier`),
  spec(`rectangle`, `rounded rect`),
  spec(`rounded_corners`, `round corners modifier`),
  spec(`slots`, `slots (sid) with their defaults`),
  spec(`star`, `star / polygon`),
  spec(`stroke`, `stroke caps, joins, dashes, trim`),
  spec(`time_remap`, `layer time remap, precomp`),
  spec(`time_stretch`, `time stretch, precomp`),
  spec(`transform`, `transform (anchor, position, scale, rotation, skew, opacity)`),
  spec(`trim_path`, `trim path`),
  { source: `spec`, path: `tests/animations/valid/gradient-stroke.json`, name: `spec-test-gradient-stroke`, covers: `gradient stroke` },
  { source: `spec`, path: `tests/animations/valid/gradient-fill.json`, name: `spec-test-gradient-fill`, covers: `gradient fill (expression stripped)` },
  { source: `spec`, path: `tests/animations/valid/ellipse-xy-pos.json`, name: `spec-test-ellipse-xy-pos`, covers: `split position (x/y)` },
  { source: `spec`, path: `tests/animations/valid/unknown-shape.json`, name: `spec-test-unknown-shape`, covers: `unknown shape type is skipped` },
  web(`adrock`, `real world: legacy keyframes, merge paths, masks, parenting, precomps`),
  web(`bacon`, `real world: legacy keyframes, merge paths, parenting`),
  web(`banner`, `real world: text (glyphs), masks, trim, dashes`, `glyphs`),
  web(`dalek`, `real world: masks, solids, nulls, parenting`),
  web(`lights`, `slider effects, parenting (expressions stripped)`),
  web(`ripple`, `gradient strokes, time remap (expressions stripped)`),
  web(`starfish`, `masks, time remap (expressions stripped)`),
  web(`bodymovin`, `real world, 1 MB: masks, merge paths, trim`),
  web(`footage/data`, `image layers (external), text (glyphs), data footage (expressions stripped)`, `glyphs`),
  skia(`skottie-auto-orient`, `auto-orient along a motion path`),
  skia(`skottie-auto-orient-2`, `auto-orient, parenting`),
  skia(`skottie-bezier-extranormal`, `spatial bezier tangents`),
  skia(`skottie-spatial-supernormal`, `spatial bezier tangents, trim`),
  skia(`skottie-transform-skew`, `skew`),
  skia(`skottie-hidden-shapes-layers`, `hidden shapes and layers (hd)`),
  skia(`skottie_fill_rule`, `even-odd fill rule`),
  skia(`skottie-gradient-opacity`, `gradient stops with opacity`),
  skia(`skottie-gradient-radial-highlight`, `radial gradient highlight (focal point)`),
  skia(`skottie-repeater`, `repeater`),
  skia(`skottie-mergepaths-effect`, `merge paths modes`),
  skia(`skottie-offsetpaths-effect`, `offset path modifier`),
  skia(`skottie-puckerbloat-effect`, `pucker/bloat`),
  skia(`skottie-trimpath-modes`, `trim simultaneous / individual`),
  skia(`skottie-trimpath-fill`, `trim applied to fills, dashes`),
  skia(`skottie-masking-opaque`, `masks: add, subtract, intersect, lighten, darken, difference, inverted`),
  skia(`skottie-masking-translucent`, `masks of every mode with opacity`),
  skia(`skottie-mask-feather`, `mask feather`),
  skia(`skottie-motion-blur-mask`, `mask feather on a solid`),
  skia(`skottie-luma-matte`, `luma matte`),
  skia(`skottie-chained-mattes`, `inverted alpha + inverted luma mattes, chained`),
  skia(`skottie-matte-blendmode`, `matte with a multiply layer`),
  skia(`skottie-matte-nonsequential`, `mattes with tp (non-adjacent matte source)`),
  skia(`skottie-matte-visible`, `visible matte source`),
  skia(`skottie-blendmode-hardmix`, `blend mode 17 (hard mix, not W3C)`),
  skia(`skottie-nested-animation`, `nested precomps`),
  skia(`skottie-time-reverse`, `negative time stretch`),
  skia(`skottie-fill-effect`, `fill effect (ef 21)`),
  skia(`skottie-tritone-effect`, `tritone effect (ef 23)`),
  skia(`skottie-dropshadow-ramp`, `drop shadow effect (ef 25)`),
  skia(`skottie-gaussian-blur`, `gaussian blur (ef 29), text (system font)`, `fonts`),
  skia(`skottie-invert-effect`, `invert effect`),
  skia(`skottie-huesaturation-effect`, `hue/saturation effect`),
  skia(`skottie-dropshadow-style`, `drop shadow layer style`),
  skia(`skottie-effects-transform`, `transform effect, image layer`),
  skia(`skottie_sample_1`, `real world strokes`),
  skia(`skottie_sample_search`, `real world: trim, strokes`),
  skia(`skottie-phonehub-connecting`, `real world: Chrome OS Phone Hub`),
  skia(`skottie-text-fill-over-stroke`, `text: fill over stroke (glyphs)`, `glyphs`),
  skia(`skottie-text-animator-1`, `text animators (glyphs)`, `glyphs`),
  skia(`skottie-text-tracking`, `text tracking (system font)`, `fonts`),
  skia(`skottie-text-valign`, `text paragraph box, alignment (system font)`, `fonts`),
  skia(`skottie-3d-parenting-nocamera`, `3D layers without a camera`),
]

const LICENCE: Record<Source, string> = {
  spec: `Community Specification License 1.0 (lottie-spec; §1.1 grants reproduction and distribution, §1.2 requires attribution: see the header)`,
  web: `MIT (lottie-web, Copyright (c) 2015 Bodymovin)`,
  skia: `BSD-3-Clause (Skia, Copyright (c) 2011 Google Inc.)`,
}
const ROOT: Record<Source, string> = { spec: specRoot, web: webRoot, skia: skiaRoot }
const URL: Record<Source, string> = {
  spec: `https://github.com/lottie/lottie-spec`,
  web: `https://github.com/airbnb/lottie-web`,
  skia: `https://skia.googlesource.com/skia`,
}

const rev = (dir: string): string => {
  try {
    return execFileSync(`git`, [`-C`, dir, `rev-parse`, `HEAD`], { encoding: `utf8` }).trim()
  } catch {
    return `unknown`
  }
}

/** Drop `"x"` (expression) strings from animated properties; returns the count. */
function stripExpressions(v: unknown): number {
  let n = 0
  if (Array.isArray(v)) for (const x of v) n += stripExpressions(x)
  else if (v && typeof v === `object`) {
    const o = v as Record<string, unknown>
    if (`k` in o && typeof o.x === `string`) {
      delete o.x
      n++
    }
    for (const k of Object.keys(o)) n += stripExpressions(o[k])
  }
  return n
}

const here = import.meta.dir
const corpus = join(here, `corpus`)
rmSync(corpus, { recursive: true, force: true })
mkdirSync(join(corpus, `images`), { recursive: true })

const rows: string[] = []
for (const p of PICKS) {
  const src = join(ROOT[p.source], p.path)
  if (statSync(src).size > 2 * 1024 * 1024) throw new Error(`${p.path}: over 2 MB`)
  const text = readFileSync(src, `utf8`)
  const json = JSON.parse(text)
  const stripped = stripExpressions(json)
  // lottie-web needs `v` (it compares versions to upgrade old files); the
  // spec makes it optional. Files without one get "5.7.0".
  // Likewise layers without `st` (spec default 0) are never shown by lottie-web.
  let versioned = typeof json.v !== `string`
  if (versioned) json.v = `5.7.0`
  for (const list of [json.layers ?? [], ...(json.assets ?? []).map((a: { layers?: unknown[] }) => a.layers ?? [])])
    for (const l of list as Record<string, unknown>[])
      if (l.st === undefined) {
        l.st = 0
        versioned = true
      }
  writeFileSync(join(corpus, `${p.name}.json`), stripped || versioned ? JSON.stringify(json) : text)
  // External images: copied next to the file (`u` + `p`, relative).
  for (const a of json.assets ?? []) {
    if (a.layers || !a.p || a.e === 1 || String(a.p).startsWith(`data:`)) continue
    const from = join(ROOT[p.source], p.path, `..`, a.u ?? ``, a.p)
    if (!existsSync(from)) {
      console.warn(`${p.name}: missing image ${from}`)
      continue
    }
    mkdirSync(join(corpus, a.u ?? ``), { recursive: true })
    copyFileSync(from, join(corpus, a.u ?? ``, a.p))
  }
  const notes = [p.covers, stripped ? `**${stripped} expression(s) stripped**` : ``, versioned ? `**lottie-web defaults added** (\`"v"\`, layer \`"st": 0\`)` : ``, p.text === `fonts` ? `text with system fonts: NOT comparable` : p.text === `glyphs` ? `text ships glyphs (chars): comparable` : ``].filter(Boolean).join(`; `)
  rows.push(`| \`${p.name}.json\` | ${p.source} \`${p.path}\` | ${p.source === `spec` ? `CSL-1.0` : p.source === `web` ? `MIT` : `BSD-3-Clause`} | ${notes} |`)
}

const md = `# eui-motion conformance corpus: sources

Generated by \`build-corpus.ts\` (do not edit by hand). Every file is redistributed
under its upstream licence:

| source | revision | licence |
|---|---|---|
${(Object.keys(ROOT) as Source[]).map((s) => `| ${s}: ${URL[s]} | \`${rev(ROOT[s])}\` | ${LICENCE[s]} |`).join(`\n`)}

**Attribution (Community Specification License 1.0 §1.2):** the \`spec-*\` files are
examples and test animations of the *Lottie Animation Format* specification,
version 1.0, by the Lottie Animation Community (LAC) Working Group, retrieved from
https://github.com/lottie/lottie-spec (\`docs/static/examples\`, \`tests/animations\`)
at the revision above. The licence's §1.1 grants everyone a copyright licence "to
reproduce, prepare derivative works of, publicly display, publicly perform, and
distribute" the materials; the stripped variants below are such derivative works.

**MIT (lottie-web):** Copyright (c) 2015 Bodymovin. Permission is hereby granted,
free of charge, to any person obtaining a copy of this software and associated
documentation files, to deal in the Software without restriction (full text:
https://github.com/airbnb/lottie-web/blob/master/LICENSE.md).

**BSD-3-Clause (Skia):** Copyright (c) 2011 Google Inc. All rights reserved.
Redistribution and use in source and binary forms, with or without modification,
are permitted provided the copyright notice, the conditions and the disclaimer
are retained (full text: https://skia.googlesource.com/skia/+/main/LICENSE).
\`images/triangle.png\` is Skia's (\`resources/skottie/images\`); \`images/img_*.jpg\`
and \`images/footage_0.json\` are lottie-web's (\`test/animations/footage/images\`).

Two edits, both applied before rendering the references, so both sides see
the same file: the spec's defaults lottie-web does not apply are written out
(\`"v": "5.7.0"\` where the version is missing, \`"st": 0\` on layers without a
start time; lottie-web hides such layers), and expressions are removed. Expressions: the engine refuses them by design, so files that carried
\`"x"\` expression strings are stored with those strings removed (marked below);
the references are rendered from the stored (stripped) file, so both sides
see the same animation.

| file | from | licence | covers / notes |
|---|---|---|---|
${rows.join(`\n`)}
`
writeFileSync(join(corpus, `SOURCES.md`), md)
console.log(`${PICKS.length} files`)
