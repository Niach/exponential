// FEED-131: Radix `Slider`'s maths and keys (single thumb, horizontal):
// `linearScale`, the value rounding, `getThumbInBoundsOffset`.

export const PAGE_KEYS = [`PageUp`, `PageDown`]
export const BACK_KEYS: Record<`from-left` | `from-right`, string[]> = { "from-left": [`Home`, `PageDown`, `ArrowDown`, `ArrowLeft`], "from-right": [`Home`, `PageDown`, `ArrowDown`, `ArrowRight`] }
export const linearScale = (input: [number, number], output: [number, number]) => (v: number) => {
  if (input[0] === input[1] || output[0] === output[1]) return output[0]
  const ratio = (output[1] - output[0]) / (input[1] - input[0])
  return output[0] + ratio * (v - input[0])
}
export const decimalCount = (v: number) => (String(v).split(`.`)[1] || ``).length
export const roundValue = (v: number, decimals: number) => {
  const r = Math.pow(10, decimals)
  return Math.round(v * r) / r
}
export const clampTo = (v: number, [lo, hi]: [number, number]) => Math.min(hi, Math.max(lo, v))
export const percentOf = (v: number, min: number, max: number) => clampTo((100 / (max - min)) * (v - min), [0, 100])
export const inBoundsOffset = (width: number, left: number, direction: number) => {
  const half = width / 2
  const offset = linearScale([0, 50], [0, half])
  return (half - offset(left) * direction) * direction
}
