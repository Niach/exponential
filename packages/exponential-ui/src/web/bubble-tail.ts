// #EXP-1876: the Box style key `tail` on the web (style.json `tail`): a
// speech-bubble tail drawn with the box as ONE outline. CSS cannot stroke a
// path with the box's own border, so the tail is two pseudo-elements laid
// over the box's edge: `::before` = the OUTER shape in the box's border
// colour (a zero-width element whose inherited border fills it), `::after`
// = the same shape inset by the border width in the box's background
// (inherited), which also covers the box's own border where the tail joins
// it: one fill, one hairline, no seam. Numbers = SPEC (every renderer):
// sideways 6 px out × 9 px down at the top corner (that corner square),
// down 9 px with a 6 px base, its tip 22 px in from the inline-end edge,
// a ~2 px rounded tip. Logical properties + `:dir(rtl)` mirror it.

export type BubbleTail = `start` | `end` | `down`

export const TAIL = { out: 6, drop: 9, inset: 22, tip: 2 } as const

export const isBubbleTail = (v: unknown): v is BubbleTail => v === `start` || v === `end` || v === `down`

const n = (v: number): string => `${Math.round(v * 1000) / 1000}`

type Pt = readonly [number, number]

/** A closed path in the pseudo-element's own box; `q` = a quadratic through
 *  the control point to the next point (the rounded tip). */
type Seg = { at: Pt } | { q: Pt; at: Pt }

const pathOf = (segs: readonly Seg[], mirrorW?: number): string => {
  const x = (p: Pt) => n(mirrorW === undefined ? p[0] : mirrorW - p[0])
  const y = (p: Pt) => n(p[1])
  return `path('${segs.map((s, i) => (`q` in s ? `Q ${x(s.q)} ${y(s.q)} ${x(s.at)} ${y(s.at)}` : `${i === 0 ? `M` : `L`} ${x(s.at)} ${y(s.at)}`)).join(` `)} Z')`
}

/** The hypotenuse 6 × 9: its unit direction and the inward normal. */
const LEN = Math.hypot(TAIL.out, TAIL.drop)
const DX = TAIL.out / LEN
const DY = TAIL.drop / LEN

/** Geometry of one tail in its pseudo-element box (LTR, start / down). */
export function tailGeometry(tail: BubbleTail, b: number): { width: number; height: number; outer: string; inner: string; outerRtl: string; innerRtl: string } {
  const { out, drop, tip } = TAIL
  // How far the hypotenuse inset by b sits from the outer one, along x at a fixed y.
  const shift = b / DY
  if (tail === `down`) {
    // Box: outer x ∈ [W-29, W-21], y ∈ [H-b-1, H+9]: local = outer - (W-29, H-b-1).
    const w = out + 2
    const h = drop + b + 1
    const ox = 1 // local x of the hypotenuse's root (outer W-28)
    const tx = 7 // local x of the tip / the straight edge (outer W-22)
    const by = b + 1 // local y of the box's outer bottom edge
    const outer: Seg[] = [
      { at: [ox, by - b] },
      { at: [tx, by - b] },
      { at: [tx, by + drop - tip] },
      { q: [tx, by + drop], at: [tx - DX * tip, by + drop - DY * tip] },
      { at: [ox, by] },
    ]
    // Inset: the straight edge by b to the left, the hypotenuse by b toward it.
    const hx = (y: number) => ox + ((y - by) * out) / drop + shift
    const tipY = by + ((tx - b - ox - shift) * drop) / out
    const inner: Seg[] = [
      { at: [0, 0] },
      { at: [tx - b, 0] },
      { at: [tx - b, tipY] },
      { at: [hx(by - b), by - b] },
      { at: [0, by - b] },
    ]
    return { width: w, height: h, outer: pathOf(outer), inner: pathOf(inner), outerRtl: pathOf(outer, w), innerRtl: pathOf(inner, w) }
  }
  // Sideways (start; end = its mirror). Box: outer x ∈ [-6, b+1], y ∈ [0, 11]: local = outer + (6, 0).
  const w = out + b + 1
  const h = drop + 2
  const sx = out // local x of the box's outer side edge
  const outer: Seg[] = [
    { at: [tip, 0] },
    { at: [sx + b, 0] },
    { at: [sx + b, drop] },
    { at: [sx, drop] },
    { at: [DX * tip, DY * tip] },
    { q: [0, 0], at: [tip, 0] },
  ]
  // Inset hypotenuse: x(y) = (y * out / drop) + shift; the top inset by b.
  const hx = (y: number) => (y * out) / drop + shift
  const innerBottom = ((w - shift) * drop) / out
  const inner: Seg[] = [
    { at: [hx(b), b] },
    { at: [w, b] },
    { at: [w, Math.min(h, innerBottom)] },
    ...(innerBottom > h ? [{ at: [hx(h), h] } as Seg] : []),
  ]
  return { width: w, height: h, outer: pathOf(outer), inner: pathOf(inner), outerRtl: pathOf(outer, w), innerRtl: pathOf(inner, w) }
}

/** The rules that draw `tail` on the element `sel` with a uniform border of
 *  `b` px. `squareCorner` = the declarations that square the tail's corner
 *  (they must win over the recipe's radius: callers emit them in their
 *  strongest layer). */
export function tailRules(sel: string, tail: BubbleTail, b: number, opts: { positioned?: boolean } = {}): { corner: string; pseudo: string } {
  const g = tailGeometry(tail, b)
  // `end` = the start geometry mirrored: its LTR shape is the mirrored one.
  const ltr = tail === `end` ? { outer: g.outerRtl, inner: g.innerRtl } : { outer: g.outer, inner: g.inner }
  const rtl = tail === `end` ? { outer: g.outer, inner: g.inner } : { outer: g.outerRtl, inner: g.innerRtl }
  const corner = tail === `start` ? `border-start-start-radius:0` : tail === `end` ? `border-start-end-radius:0` : ``
  const place =
    tail === `down`
      ? `inset-inline-end:${n(TAIL.inset - 1 - b)}px;bottom:${n(-(TAIL.drop + b))}px`
      : `${tail === `start` ? `inset-inline-start` : `inset-inline-end`}:${n(-(TAIL.out + b))}px;top:${n(-b)}px`
  const both = `${sel}::before,${sel}::after`
  const base =
    `${both}{content:"";position:absolute;${place};width:${n(g.width)}px;height:${n(g.height)}px;pointer-events:none;box-sizing:border-box}` +
    // The border colour: a zero-width box whose inline-start border fills it.
    `${sel}::before{box-sizing:content-box;width:0;border:0 solid;border-inline-start-width:${n(g.width)}px;border-color:inherit;clip-path:${ltr.outer}}` +
    `${sel}::after{background:inherit;clip-path:${ltr.inner}}` +
    `${sel}:dir(rtl)::before{clip-path:${rtl.outer}}` +
    `${sel}:dir(rtl)::after{clip-path:${rtl.inner}}`
  const pos = opts.positioned ? `` : `${sel}{position:relative}`
  return { corner: corner ? `${sel}{${corner}}` : ``, pseudo: pos + base }
}
