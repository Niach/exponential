// N-12: THE stack geometry ×4, the toast's (fixtures/toast-stack.json
// `geometry`) shared with the Dock pile (fixtures/dock.json `stack`): the
// newest item in front, the older ones peeking out behind it by `peek` and
// shrinking by `scaleStep` per rank, `visible` of them at most; expanded =
// every item at full size, `gap` apart. TS reference; desktop
// `toast::stack_layout`, iOS ExpCore `ToastStack`, Android
// `domain/ToastStack.kt` replay toast-stack.json's cases with the toast's
// constants and draw the Dock with dock.json's.

import toastFixture from "../fixtures/toast-stack.json" with { type: "json" }

export interface StackConstants {
  /** How far an older item peeks out behind the one in front of it. */
  peek: number
  /** How much smaller each rank behind the front draws. */
  scaleStep: number
  /** How many items draw at all while collapsed. */
  visible: number
  /** The space between items while expanded. */
  gap: number
}

export interface StackItem {
  /** The item's top inside the stack box. */
  offset: number
  /** Its width factor. */
  scale: number
  /** Whether it draws at all. */
  visible: boolean
}

export interface StackLayout {
  /** The stack box's height. */
  height: number
  /** Per item, oldest first. */
  items: StackItem[]
}

/** The toast's numbers (toast-stack.json `constants`). */
export const TOAST_STACK: StackConstants = {
  peek: toastFixture.constants.peek,
  scaleStep: toastFixture.constants.scaleStep,
  visible: toastFixture.constants.visible,
  gap: toastFixture.constants.gap,
}

const round = (value: number) => Math.round(value * 1e4) / 1e4

/** `heights` oldest first (the last is the front). `anchoredBottom` = the
 *  box grows upward from its bottom edge (a bottom stack); otherwise from
 *  its top. */
export function stackLayout(
  heights: readonly number[],
  { expanded, anchoredBottom }: { expanded: boolean; anchoredBottom: boolean },
  constants: StackConstants
): StackLayout {
  const n = heights.length
  const rank = (i: number) => n - 1 - i
  // Distance of each item's anchor-side edge from the box's anchor edge.
  const lead = heights.map((_, i) =>
    expanded
      ? heights.slice(i + 1).reduce((sum, h) => sum + h + constants.gap, 0)
      : constants.peek * rank(i)
  )
  const height = heights.reduce((max, h, i) => Math.max(max, lead[i]! + h), 0)
  const items = heights.map((h, i) => {
    const r = rank(i)
    const offset = anchoredBottom ? height - lead[i]! - h : lead[i]!
    if (expanded) return { offset: round(offset), scale: 1, visible: true }
    return {
      offset: round(offset),
      scale: round(1 - constants.scaleStep * Math.min(r, constants.visible - 1)),
      visible: r < constants.visible,
    }
  })
  return { height: round(height), items }
}
