// FEED-131: the `plain` lane's jsdom parity. Every specimen of
// harness/primitives/plain.tsx (the browser gate's twins) renders on both
// sides, whole <body> compared; plus the cases a static specimen cannot
// show (the copy button after a copy, a live prop change).

import { createElement, type ReactElement } from "react"
import { createSignal } from "solid-js"
import { SPECIMENS as SOLID } from "../../harness/primitives/plain"
import { expectElementParity, renderReactElement, renderSolidElement, serializeBody, serializeDom } from "../test/parity"
import { CodeBlockCopyButton, Progress } from "./index"

// The React side's JSX stays out of this package's Solid transform: loaded
// at run time (see src/test/parity.ts).
const REACT_SPECIMENS = `${import.meta.dirname}/../../../exponential-ui-react/harness/primitives/plain.tsx`
const REACT_PRIMITIVES = `@exponential_app/ui-react/primitives`
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyComponent = (props: any) => ReactElement
let react: Record<string, () => ReactElement>
let R: Record<string, AnyComponent>

beforeAll(async () => {
  react = ((await import(/* @vite-ignore */ REACT_SPECIMENS)) as { SPECIMENS: Record<string, () => ReactElement> }).SPECIMENS
  R = (await import(/* @vite-ignore */ REACT_PRIMITIVES)) as Record<string, AnyComponent>
})

describe(`plain primitives: DOM parity with React`, () => {
  it(`both harnesses define the same specimen ids`, () => {
    expect(Object.keys(SOLID).sort()).toEqual(Object.keys(react).sort())
  })

  for (const id of Object.keys(SOLID)) {
    it(id, async () => {
      await expectElementParity({ react: react[id]!, solid: SOLID[id]! })
    })
  }

  it(`the copy button after a copy`, async () => {
    Object.assign(navigator, { clipboard: { writeText: () => Promise.resolve() } })
    const press = async () => {
      document.querySelector<HTMLButtonElement>(`[data-slot="code-block-copy-button"]`)!.click()
      await new Promise((r) => setTimeout(r, 0))
    }
    
    const r = await renderReactElement(() => createElement(R.CodeBlockCopyButton!, { value: `x` }))
    const { act } = await import(`react`)
    await act(press)
    const reactDom = serializeBody()
    r.cleanup()
    const s = await renderSolidElement(() => <CodeBlockCopyButton value="x" />)
    await press()
    const solidDom = serializeBody()
    s.cleanup()
    expect(reactDom).toContain(`data-copied="true"`)
    expect(solidDom).toBe(reactDom)
  })

  it(`progress follows a live value`, async () => {
    const [value, setValue] = createSignal<number | null>(10)
    const s = await renderSolidElement(() => <Progress value={value()} />)
    setValue(null)
    const solidDom = serializeDom(s.container)
    s.cleanup()
    const r = await renderReactElement(() => createElement(R.Progress!, { value: null }))
    const reactDom = serializeDom(r.container)
    r.cleanup()
    expect(reactDom).toContain(`data-state="indeterminate"`)
    expect(solidDom).toBe(reactDom)
  })
})
