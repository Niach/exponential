// Round 4 motion: what the web harnesses report for one fixtures/
// animations.json `presence` scenario (conformance/browser-suites.ts): the
// container's painted children after a step, ghosts = `data-xui-exit`, the
// enter offset = the computed `animation-delay` (ms; reduced motion zeroes
// it). Each harness drives the steps through its own surface.

import type { FlatComponent } from "../types"

export interface PresenceScenario {
  name: string
  container: string
  reducedMotion?: boolean
  windowed?: boolean
  first?: number
  viewport?: { width: number; height: number }
  components: FlatComponent[]
  data: Record<string, unknown>
  steps: { data?: { path: string; value: unknown }; components?: FlatComponent[]; finishExit?: string; expect: PresencePainted[] }[]
}

export interface PresencePainted {
  id: string
  exiting: boolean
  delayMs: number
}

const ms = (v: string): number => {
  const first = v.split(`,`)[0]!.trim()
  return first.endsWith(`ms`) ? Number.parseFloat(first) : Number.parseFloat(first) * 1000 || 0
}

/** The container's own painted child nodes (a List wraps each in its row box). */
export function readPresence(scope: ParentNode, container: string, first?: number): PresencePainted[] {
  const root = scope.querySelector<HTMLElement>(`[data-xui-id="${container}"]`)
  if (!root) return []
  const els = Array.from(root.querySelectorAll<HTMLElement>(`[data-xui-id]`)).filter((el) => el.parentElement?.closest(`[data-xui-id]`) === root)
  const out = els.map((el) => ({ id: el.dataset.xuiId!, exiting: el.hasAttribute(`data-xui-exit`), delayMs: Math.round(ms(getComputedStyle(el).animationDelay || `0ms`) * 1000) / 1000 }))
  return first ? out.slice(0, first) : out
}
