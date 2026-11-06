// #VAPP-258 step 5 (catalog/host.json `markdown.options`): a host's render
// options. Hard breaks break every line, the chat density rides as the
// Markdown root's `density` recipe prop (the theme's chat rule), `selectable:
// false` turns selection off, and a streamed prefix holds its unfinished
// tail back. No options = as before.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, render } from "@testing-library/react"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import { ExponentialSurface } from "./surface"
import type { HostPlugin } from "./host"

afterEach(cleanup)

const root = (text: string) => reduceNested({ id: `md`, component: `Markdown`, props: { text } }, { catalogId: CORE_CATALOG_ID }).root
const paint = (text: string, host?: HostPlugin) => render(<ExponentialSurface root={root(text)} theme="neutral" id="opts" host={host} />).container

describe(`#VAPP-258 Markdown options`, () => {
  it(`no options paint as before`, () => {
    const c = paint(`one\ntwo`)
    expect(c.querySelector(`[data-xui-c="Markdown"]`)!.hasAttribute(`data-r-density`)).toBe(false)
    expect(c.querySelector(`.xui-Markdown-paragraph`)!.querySelectorAll(`br`).length).toBe(0)
    expect(c.querySelector(`.xui-md`)!.hasAttribute(`data-selectable`)).toBe(false)
  })

  it(`hard breaks, the chat density and no selection`, () => {
    const c = paint(`one\ntwo\nthree`, { markdownOptions: { breaks: `hard`, density: `chat`, selectable: false } })
    expect(c.querySelector(`.xui-Markdown-paragraph`)!.querySelectorAll(`br`).length).toBe(2)
    expect(c.querySelector(`[data-xui-c="Markdown"]`)!.getAttribute(`data-r-density`)).toBe(`chat`)
    expect(c.querySelector(`.xui-md`)!.getAttribute(`data-selectable`)).toBe(`false`)
  })

  it(`a streamed prefix paints its unfinished tail as plain text`, () => {
    const c = paint(`Done **para**\n\nSay **bo and [the do`, { markdownOptions: { streaming: true } })
    const paragraphs = c.querySelectorAll(`.xui-Markdown-paragraph`)
    expect(paragraphs[0]!.querySelector(`strong`)!.textContent).toBe(`para`)
    expect(paragraphs[1]!.textContent).toBe(`Say bo and the do`)
    expect(paragraphs[1]!.querySelector(`strong`)).toBeNull()
  })

  it(`an open fence paints as an open code block`, () => {
    const c = paint("Intro\n\n```ts\nconst a = 1\n``", { markdownOptions: { streaming: true } })
    expect(c.querySelector(`.xui-Markdown-codeBlock`)!.textContent).toBe(`const a = 1`)
  })
})
