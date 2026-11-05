import type { ReferenceSource } from "../sync-reference.ts"

/* Libraries area (genUI + Motion): reference pages generated from repo
   markdown. The genUI component pages are generated from the catalog in
   eui-components.ts; the other libraries list theirs in libraries-other.ts. */

/** Drops ticket ids from headings: `## Engine (M1, EXP-1602)` → `## Engine`. */
const plainHeadings = (md: string) => md.replace(/^(#+ .*?)\s*\((?:M\d+, )?[A-Z]+-\d+\)\s*$/gm, `$1`)

/** Cuts everything from the heading `## <title>` to the end. */
const cutFrom = (title: string) => (md: string) => {
  const at = md.indexOf(`\n## ${title}`)
  return at < 0 ? md : md.slice(0, at + 1)
}

/** Every field the dialect names (docs/spec/motion.md §3 in one table). */
const DIALECT_FIELDS = `
## Fields

Anything else Lottie can do rides along as raw keys under \`lottie\` on the root, a layer, a transform or a shape. The dialect is lossless both ways: \`exponential motion decompile\` turns a Lottie into the dialect and \`exponential motion compile\` turns it back.

| Where | Fields |
|---|---|
| Root | \`size\` [w, h] · \`duration\` · \`fps\` (60) · \`start\` · \`time\` ("ms" \\| "frames") · \`layers\` · \`assets\` {id: {layers} \\| {image, size}} · \`markers\` [{name, time, duration}] |
| Every layer | \`type\` · \`id\` · \`name\` · \`parent\` · \`in\` / \`out\` / \`start\` · \`stretch\` · \`hidden\` · \`autoOrient\` · \`blend\` · \`transform\` · \`matte\` {source, mode} · \`masks\` [{path, mode, inverted, opacity, expansion, feather}] |
| Layer types | \`shape\` {shapes} · \`solid\` {color, size} · \`image\` {asset} · \`precomp\` {asset, size, timeRemap} · \`null\` · \`text\` {text: {text, font, size, color, strokeColor, strokeWidth, align, tracking, lineHeight, box, animators}} |
| \`transform\` | \`anchor\` · \`position\` (or \`positionX\` / \`positionY\`) · \`scale\` % · \`rotation\` ° clockwise · \`opacity\` 0–100 · \`skew\` · \`skewAxis\` |
| Geometry | \`rect\` {position, size, radius} · \`ellipse\` {position, size} · \`star\` {points, outerRadius, innerRadius, outerRoundness, innerRoundness, rotation} · \`polygon\` · \`path\` {d} |
| Styles | \`fill\` {color, opacity, rule} · \`stroke\` {color, width, cap, join, miterLimit, dash, dashOffset} · \`gradient-fill\` / \`gradient-stroke\` {kind, start, end, stops, highlight, highlightAngle} |
| Modifiers | \`trim\` {start, end, offset, mode} · \`repeater\` {copies, offset, composite, transform} · \`round-corners\` · \`pucker-bloat\` · \`zig-zag\` · \`offset-path\` · \`twist\` · \`merge\` {mode} |
| Groups | \`group\` {shapes, transform}: a style paints every shape before it in its group |
| Values | a number, an array, a colour, path data, or {keys: [{t, v, ease, end, to, ti}]} |
| \`ease\` | \`linear\` (default) · \`ease\` · \`ease-in\` · \`ease-out\` · \`ease-in-out\` · \`hold\` · [x1, y1, x2, y2] |
| Colours | \`#rgb\` · \`#rrggbb\` · \`#rrggbbaa\` · [r, g, b, a] 0–1 · \`$color.<token>\` · {light, dark} |
`

export const REFERENCE: ReferenceSource[] = [
  {
    source: `docs/spec/motion.md`,
    slug: `docs/sdk/motion/reference`,
    title: `Motion specification`,
    description: `The Motion component in full: sources, limits, the dialect, the engine, the display list, playback, interactivity and tooling.`,
  },
  {
    source: `packages/exponential-ui/eval/motion/guide.md`,
    slug: `docs/sdk/motion/reference/dialect`,
    title: `Motion dialect`,
    description: `The dialect a person or a model writes: the guide every model gets in its prompt, then every field it names.`,
    transform: (md) => `The text below is the whole guide a model gets in its prompt (the \`Motion\` entry of \`catalogPrompt()\`, about 600 tokens).\n\n## The prompt guide\n\n${md.trim()}\n${DIALECT_FIELDS}`,
  },
  {
    source: `apps/desktop/crates/eui-motion/README.md`,
    slug: `docs/sdk/motion/reference/engine`,
    title: `Motion engine and players`,
    description: `eui-motion: the display list, packed frames, the engine, authoring, the painting and interactivity contracts every renderer keeps, and the limits.`,
    transform: (md) => plainHeadings(cutFrom(`M0 spike`)(md)),
  },
]
