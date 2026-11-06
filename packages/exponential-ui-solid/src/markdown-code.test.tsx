// #VAPP-258 on Solid (React's markdown-code.test.tsx): a fenced block whose fence names
// a catalog language paints CodeBlock's tokens (the catalog's ONE tokenizer)
// as `CodeBlock/token {kind}` spans under the surface's theme scope, so the
// CodeBlock component's colours reach Markdown; an unknown or empty language
// stays plain text; the text (so the box) is the same either way.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, reduceNested, tokenizeCode } from "@exponential_app/eui"
import { ExponentialSurface } from "./surface"

afterEach(cleanup)

const paint = (text: string) =>
  render(() => <ExponentialSurface root={reduceNested({ id: `md`, component: `Markdown`, props: { text } }, { catalogId: CORE_CATALOG_ID }).root} theme="neutral" id="code" />).container

describe(`#VAPP-258 Markdown code blocks are highlighted`, () => {
  it(`a ts fence paints CodeBlock's token kinds in the CodeBlock/token recipe`, () => {
    const c = paint("```ts\nconst a = 1 // one\n```")
    const pre = c.querySelector(`.xui-Markdown-codeBlock`)!
    expect(pre.textContent).toBe(`const a = 1 // one`)
    const spans = [...pre.querySelectorAll(`.xui-CodeBlock-token`)]
    const want = tokenizeCode(`const a = 1 // one`, `ts`).flat().filter((t) => t.kind !== `plain`)
    expect(spans.map((s) => [s.getAttribute(`data-r-kind`), s.textContent])).toEqual(want.map((t) => [t.kind, t.text]))
    expect(spans.map((s) => s.getAttribute(`data-r-kind`))).toEqual(expect.arrayContaining([`keyword`, `number`, `comment`]))
    // The theme's scope rides on every span: the recipe's selector matches.
    const scope = c.querySelector(`.xui-surface`)!.className.split(` `).find((k) => k.startsWith(`xui-t-`))
    expect(scope).toBeDefined()
    for (const s of spans) expect(s.classList.contains(scope!)).toBe(true)
  })

  it(`an unknown or empty language is plain text`, () => {
    for (const text of ["```\nconst a = 1\n```", "```klingon\nconst a = 1\n```", "```plain\nconst a = 1\n```"]) {
      const pre = paint(text).querySelector(`.xui-Markdown-codeBlock`)!
      expect(pre.querySelectorAll(`.xui-CodeBlock-token`).length).toBe(0)
      expect(pre.textContent).toBe(`const a = 1`)
      cleanup()
    }
  })

  it(`a multi-line fence keeps its newlines`, () => {
    const pre = paint("```kotlin\nval a = 1\nval b = 2\n```").querySelector(`.xui-Markdown-codeBlock`)!
    expect(pre.textContent).toBe(`val a = 1\nval b = 2`)
    expect(pre.querySelectorAll(`.xui-CodeBlock-token[data-r-kind="keyword"]`).length).toBe(2)
  })
})
