// genUI round 4 (catalog/style.json `backgroundGradient` radial and
// `backgroundImage`): the geometry every painter shares, the reference the
// Rust core (`style::radial_radius`, `radial_t`, `picture_rect`), Swift and
// Kotlin replay against fixtures/box-backgrounds.json.

/** A radial gradient's 100% radius in a `w`×`h` box about the centre
 *  fractions: the distance to the FARTHEST corner (CSS `circle
 *  farthest-corner`). */
export function radialRadius(w: number, h: number, cx: number, cy: number): number {
  const x = cx * w
  const y = cy * h
  return Math.hypot(Math.max(x, w - x), Math.max(y, h - y))
}

/** Where `(px, py)` sits on the radial gradient: |P − C| / R (0 = centre,
 *  1 = the farthest corner); 0 when R is 0. */
export function radialT(w: number, h: number, cx: number, cy: number, px: number, py: number): number {
  const r = radialRadius(w, h, cx, cy)
  return r > 0 ? Math.hypot(px - cx * w, py - cy * h) / r : 0
}

/** Where a picture of `image` size lands in a `box` for `fit` cover |
 *  contain and the focal fractions (CSS `background-position` with
 *  `background-size` cover|contain; Image's `focal_bounds`): `[x, y, w, h]`
 *  relative to the box. */
export function pictureRect(fit: string, box: readonly [number, number], image: readonly [number, number], focal: readonly [number, number]): [number, number, number, number] {
  const [w, h] = box
  const [iw, ih] = image
  if (iw <= 0 || ih <= 0) return [0, 0, w, h]
  const scale = fit === `contain` ? Math.min(w / iw, h / ih) : Math.max(w / iw, h / ih)
  const dw = iw * scale
  const dh = ih * scale
  const clamp = (v: number) => Math.max(0, Math.min(1, v))
  return [(w - dw) * clamp(focal[0]), (h - dh) * clamp(focal[1]), dw, dh]
}
