/**
 * YO-30: the reference for `fixtures/text-clusters.json`. Text lays out per
 * grapheme cluster (UAX #29 extended): font fallback picks one face per
 * cluster (a combining mark shares its base's face), and CSS
 * `letter-spacing` is added once per cluster (a mark and a bidi control add
 * none).
 */

const segmenter = new Intl.Segmenter(undefined, { granularity: `grapheme` })

/** The invisible bidi controls (no letter-spacing of their own). */
const BIDI_CONTROL = /^[؜‎‏‪-‮⁦-⁩]+$/u

/** The grapheme clusters of `text`, in order. */
export function textClusters(text: string): string[] {
  return Array.from(segmenter.segment(text), (s) => s.segment)
}

/** How many times `letter-spacing` is added to `text`. */
export function trackedClusters(text: string): number {
  return textClusters(text).filter((c) => !BIDI_CONTROL.test(c)).length
}
