// #EXP-1804: CodeBlock `follow`: a scrolling block stays at its bottom
// while `code` grows (a live log), stops once the person scrolled up and
// follows again back at the bottom; without `follow` nothing moves.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@testing-library/react"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import { ExponentialSurface } from "../surface"

afterEach(cleanup)

const LINE = 20
const VISIBLE = 3
const log = (n: number) => Array.from({ length: n }, (_, i) => `\u001b[32m✓\u001b[0m step ${i + 1}`).join(`\n`)
const root = (n: number, follow: boolean) => reduceNested({ id: `log`, component: `CodeBlock`, props: { code: log(n), language: `ansi`, maxLines: VISIBLE, follow } } as NestedNode, { catalogId: CORE_CATALOG_ID }).root

/** jsdom lays nothing out: the body's heights follow its line count and its
 *  scrollTop is a plain clamped number. */
function fakeScroll(body: HTMLElement) {
  let top = 0
  const max = () => Math.max(0, body.querySelectorAll(`[data-line]`).length * LINE - VISIBLE * LINE)
  Object.defineProperty(body, `scrollHeight`, { configurable: true, get: () => body.querySelectorAll(`[data-line]`).length * LINE })
  Object.defineProperty(body, `clientHeight`, { configurable: true, get: () => VISIBLE * LINE })
  Object.defineProperty(body, `scrollTop`, { configurable: true, get: () => top, set: (v: number) => void (top = Math.min(Math.max(0, v), max())) })
  return { max, scrollTo: (v: number) => ((body.scrollTop = v), fireEvent.scroll(body)) }
}

describe(`CodeBlock follow`, () => {
  it(`sticks to the bottom as lines arrive, stops when scrolled up, resumes at the bottom`, () => {
    const { container, rerender } = render(<ExponentialSurface root={root(2, true)} theme="neutral" id="cf" />)
    const body = container.querySelector(`.xui-codeblock-scroll`) as HTMLElement
    const s = fakeScroll(body)
    rerender(<ExponentialSurface root={root(6, true)} theme="neutral" id="cf" />)
    expect(body.scrollTop).toBe(s.max())
    expect(body.scrollTop).toBe(60)
    s.scrollTo(20)
    rerender(<ExponentialSurface root={root(9, true)} theme="neutral" id="cf" />)
    expect(body.scrollTop).toBe(20)
    s.scrollTo(s.max())
    rerender(<ExponentialSurface root={root(12, true)} theme="neutral" id="cf" />)
    expect(body.scrollTop).toBe(s.max())
    expect(body.scrollTop).toBe(180)
  })
  it(`without follow the view stays where it is`, () => {
    const { container, rerender } = render(<ExponentialSurface root={root(2, false)} theme="neutral" id="cn" />)
    const body = container.querySelector(`.xui-codeblock-scroll`) as HTMLElement
    fakeScroll(body)
    rerender(<ExponentialSurface root={root(6, false)} theme="neutral" id="cn" />)
    expect(body.scrollTop).toBe(0)
  })
})
