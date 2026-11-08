/* The closing band's motif: eight curves y = e^(kx), one per k, from the
   bottom-left corner up the right edge, sampled into SVG paths on a
   1440 x 480 box (CtaBand.astro; the styleguide's site-cta-band). */
export const CTA_BOX = { width: 1440, height: 480 } as const

export const CTA_CURVES: string[] = Array.from({ length: 8 }, (_, i) => {
  const { width: W, height: H } = CTA_BOX
  const k = 2.4 + i * 0.55
  const top = 1 / Math.exp(k)
  const points = Array.from({ length: 49 }, (_, s) => {
    const x = s / 48
    const y = (Math.exp(k * x) * top - top) / (1 - top)
    return `${(x * W).toFixed(1)},${(H - y * (H - 24 - i * 18)).toFixed(1)}`
  })
  return `M${points.join(`L`)}`
})
