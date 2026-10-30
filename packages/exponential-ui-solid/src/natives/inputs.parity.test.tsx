// FEED-128: DOM parity of natives/inputs.tsx with React's (every catalog
// fixture case, forced states, dark + rtl, inside a Form, invalid fields,
// and interactions replayed on both surfaces).

import { describe, it } from "vitest"
import type { NestedNode } from "@exponential_app/eui"
import { expectParity, fixtureCases } from "./inputs-parity"

const COMPONENTS = [`Input`, `Textarea`, `NumberField`, `Checkbox`, `Radio`, `Switch`, `Slider`, `Composer`] as const
const STATES = [`hover`, `focus`, `pressed`, `checked`, `open`, `disabled`, `invalid`]

const fire = (el: Element | null, type: string, init: Record<string, unknown> = {}) => {
  const Ctor = type.startsWith(`key`) ? KeyboardEvent : type.startsWith(`pointer`) ? (globalThis.PointerEvent ?? MouseEvent) : type === `click` || type.startsWith(`mouse`) ? MouseEvent : type === `input` ? InputEvent : Event
  el!.dispatchEvent(new (Ctor as typeof Event)(type, { bubbles: true, cancelable: true, ...init }))
}
const typeInto = (el: Element | null, value: string) => {
  const input = el as HTMLInputElement
  Object.getOwnPropertyDescriptor(Object.getPrototypeOf(input), `value`)!.set!.call(input, value)
  fire(input, `input`)
}

describe(`inputs: catalog fixture cases`, () => {
  for (const c of fixtureCases(COMPONENTS)) it(c.name, async () => expectParity({ tree: c.node, theme: `neutral` }))
})

describe(`inputs: forced states, dark + rtl`, () => {
  for (const component of COMPONENTS) {
    const example = fixtureCases([component])[0]
    it(`${component}: every forced state`, async () => expectParity({ tree: example.node, theme: `exponential`, states: STATES }))
    it(`${component}: dark + rtl (orange)`, async () => expectParity({ tree: example.node, theme: `orange`, mode: `dark`, direction: `rtl` }))
  }
})

const form: NestedNode = {
  id: `f`,
  component: `Form`,
  props: { summary: true },
  children: [
    { id: `in`, component: `Input`, props: { label: `Title`, name: `title`, description: `Short`, value: { path: `/title` }, checks: [{ condition: { call: `required`, args: { value: { path: `/title` } } }, message: `Required` }], validateOn: `change` } },
    { id: `ta`, component: `Textarea`, props: { label: `Body`, name: `body`, value: `Hello`, autosize: true, maxRows: 6 } },
    { id: `nf`, component: `NumberField`, props: { label: `Seats`, name: `seats`, value: { path: `/seats` }, min: 1, max: 50, step: 0.5, unit: `seats` } },
    { id: `cb`, component: `Checkbox`, props: { label: `Agree`, name: `agree`, description: `Terms`, checked: { path: `/agree` } } },
    { id: `sw`, component: `Switch`, props: { label: `Auto`, name: `auto`, description: `Hourly`, checked: false } },
    { id: `ra`, component: `Radio`, props: { label: `Plan`, name: `plan`, options: [{ label: `Free`, value: `free`, icon: `star` }, { label: `Team`, value: `team`, disabled: true }], value: `team` } },
    { id: `sl`, component: `Slider`, props: { label: `Volume`, name: `vol`, min: 0, max: 10, step: 0.5, value: 2.5 } },
    { id: `co`, component: `Composer`, props: { placeholder: `Ask`, value: `draft`, submitLabel: `Go`, attachments: true } },
    { id: `go`, component: `Button`, props: { label: `Save`, type: `submit` } },
  ],
} as unknown as NestedNode

describe(`inputs: inside a Form`, () => {
  it(`every control, bound values, bubble inputs`, async () => expectParity({ tree: form, data: { title: ``, seats: 2, agree: true }, theme: `exponential` }))
  it(`submit shows every field's errors (invalid)`, async () =>
    expectParity({ tree: form, data: { title: ``, seats: 2, agree: true }, theme: `exponential` }, (root) => {
      fire(root.querySelector(`form`), `submit`)
    }))
  it(`disabled Form`, async () => expectParity({ tree: { ...form, props: { disabled: true } } as NestedNode, data: { title: `x`, seats: 2, agree: false } }))
})

const one = (node: Record<string, unknown>, data?: Record<string, unknown>) => ({ tree: node as unknown as NestedNode, data, theme: `neutral` })

describe(`inputs: interactions replayed on both`, () => {
  it(`Input: typing, focus, Enter`, async () =>
    expectParity(one({ id: `i`, component: `Input`, props: { label: `T`, name: `t`, value: { path: `/t` } } }, { t: `` }), (root) => {
      const input = root.querySelector(`input`)
      fire(input, `focusin`)
      ;(input as HTMLInputElement).focus()
      typeInto(input, `abc`)
      fire(input, `keydown`, { key: `Enter` })
    }))
  it(`NumberField: steppers and keys`, async () =>
    expectParity(one({ id: `n`, component: `NumberField`, props: { label: `N`, value: 3, min: 1, max: 5 } }), (root) => {
      fire(root.querySelector(`[data-xui-part="NumberField/increment"]`), `click`)
    }, (root) => fire(root.querySelector(`[data-xui-part="NumberField/decrement"]`), `click`), (root) => fire(root.querySelector(`input`), `keydown`, { key: `ArrowUp` }), (root) => fire(root.querySelector(`input`), `keydown`, { key: `End` })))
  it(`Checkbox: click toggles`, async () =>
    expectParity(one({ id: `c`, component: `Checkbox`, props: { label: `C`, checked: false } }), (root) => {
      fire(root.querySelector(`button`), `click`)
    }))
  it(`Switch: the row toggles`, async () =>
    expectParity(one({ id: `s`, component: `Switch`, props: { label: `S`, checked: false } }), (root) => {
      fire(root.querySelector(`[data-xui-id="s"]`), `click`)
    }))
  it(`Radio: click picks`, async () =>
    expectParity(one({ id: `r`, component: `Radio`, props: { label: `R`, options: [{ label: `A`, value: `a` }, { label: `B`, value: `b` }], value: `a` } }), (root) => {
      const b = root.querySelectorAll(`[role="radio"]`)[1]
      fire(b, `mousedown`)
      fire(b, `click`)
    }))
  it(`Slider: keys`, async () =>
    expectParity(one({ id: `sl`, component: `Slider`, props: { label: `V`, min: 0, max: 100, step: 5, value: 40 } }), (root) => {
      fire(root.querySelector(`[role="slider"]`), `keydown`, { key: `ArrowRight` })
    }, (root) => fire(root.querySelector(`[role="slider"]`), `keydown`, { key: `PageDown` }), (root) => fire(root.querySelector(`[role="slider"]`), `keydown`, { key: `Home` })))
  it(`Composer: type then busy-less submit`, async () =>
    expectParity(one({ id: `co`, component: `Composer`, props: { placeholder: `Ask` } }), (root) => {
      typeInto(root.querySelector(`textarea`), `hi`)
    }))
})
