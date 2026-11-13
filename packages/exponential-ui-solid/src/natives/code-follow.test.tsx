// #EXP-1804 (React's natives/code-follow.test.tsx): CodeBlock `follow`: a
// scrolling block stays at its bottom while `code` grows (a live log),
// stops once the person scrolled up and follows again back at the bottom;
// without `follow` nothing moves.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@solidjs/testing-library"
import type { NestedNode } from "@exponential_app/eui"
import { createSurface, type SurfaceState } from "../create-surface"
import { ExponentialSurface } from "../surface"

afterEach(cleanup)

const LINE = 20
const VISIBLE = 3
const log = (n: number) => Array.from({ length: n }, (_, i) => `\u001b[32m✓\u001b[0m step ${i + 1}`).join(`\n`)
const node = (follow: boolean) => ({ id: `log`, component: `CodeBlock`, props: { code: { "@path": `/log` }, language: `ansi`, maxLines: VISIBLE, follow } }) as unknown as NestedNode

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

function mount(follow: boolean) {
  let state!: SurfaceState
  const { container } = render(() => {
    state = createSurface({ surfaceId: `cf`, data: { log: log(2) }, initial: node(follow) })
    return <ExponentialSurface surface={state} theme="neutral" id="cf" />
  })
  const body = container.querySelector(`.xui-codeblock-scroll`) as HTMLElement
  return { body, s: fakeScroll(body), grow: (n: number) => state.setData(`/log`, log(n)) }
}

describe(`CodeBlock follow`, () => {
  it(`sticks to the bottom as lines arrive, stops when scrolled up, resumes at the bottom`, () => {
    const { body, s, grow } = mount(true)
    grow(6)
    expect(body.scrollTop).toBe(60)
    s.scrollTo(20)
    grow(9)
    expect(body.scrollTop).toBe(20)
    s.scrollTo(s.max())
    grow(12)
    expect(body.scrollTop).toBe(180)
  })
  it(`without follow the view stays where it is`, () => {
    const { body, grow } = mount(false)
    grow(6)
    expect(body.scrollTop).toBe(0)
  })
})
