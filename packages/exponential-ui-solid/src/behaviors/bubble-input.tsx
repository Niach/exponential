// FEED-131: Radix's hidden form inputs (`BubbleInput`, `BubbleSelect`):
// rendered only inside a <form> (`createInForm`, Radix's `isFormControl`),
// a state change replayed on them with the native setter + a bubbling event
// so <form> listeners see it.

import { For, createEffect, createSignal, on, onMount, untrack, type Accessor, type JSX } from "solid-js"
import { bool, str } from "@exponential_app/eui/web"
import { css } from "../style"
import { createElementSize } from "./element-size"

/** Radix's `isFormControl`: the control sits in a <form> (decided once
 *  mounted; Radix renders its bubble input only then). */
export function createInForm(el: Accessor<HTMLElement | undefined>): Accessor<boolean> {
  const [inForm, setInForm] = createSignal(false)
  onMount(() => setInForm(Boolean(el()?.closest(`form`))))
  return inForm
}

/** Radix's bubble-input effect: a state change replays on the hidden input
 *  (the native setter + a bubbling event) so <form> listeners see it. */
export function bubbleOnChange<T>(input: () => HTMLInputElement | HTMLSelectElement | undefined, value: Accessor<T>, apply: (el: HTMLInputElement | HTMLSelectElement, v: T) => void, event: string, bubbles: () => boolean = () => true) {
  createEffect(
    on(
      value,
      (v) => {
        const el = input()
        if (!el) return
        apply(el, v)
        el.dispatchEvent(new Event(event, { bubbles: bubbles() }))
      },
      { defer: true }
    )
  )
}

export const setChecked = (el: HTMLInputElement | HTMLSelectElement, v: boolean) => Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, `checked`)?.set?.call(el, v)

/** Radix's hidden checkbox/radio bubble input (inside a <form> only). */
export function BubbleCheck(props: { type: `checkbox` | `radio`; control: HTMLElement | undefined; checked: boolean; name?: string; value: string; disabled: boolean; transformFirst: boolean; bubbles: () => boolean }): JSX.Element {
  let input: HTMLInputElement | undefined
  const size = createElementSize(() => props.control)
  const initial = untrack(() => props.checked)
  bubbleOnChange(() => input, () => props.checked, setChecked, `click`, props.bubbles)
  const style = () => {
    const s = size()
    const box = s ? { width: s.width, height: s.height } : {}
    const fixed = { position: `absolute`, pointerEvents: `none`, opacity: 0, margin: 0 }
    return css(props.transformFirst ? { transform: `translateX(-100%)`, ...box, ...fixed } : { ...box, ...fixed, transform: `translateX(-100%)` })
  }
  return <input ref={input} type={props.type} aria-hidden="true" attr:checked={initial ? `` : undefined} name={props.name} value={props.value} disabled={props.disabled} tabindex={-1} style={style()} />
}

export function SliderBubble(props: { name?: string; value: number }): JSX.Element {
  let input: HTMLInputElement | undefined
  const initial = untrack(() => String(props.value))
  bubbleOnChange(() => input, () => props.value, (el, v) => Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, `value`)?.set?.call(el, v), `input`)
  return <input ref={input} style={css({ display: `none` })} name={props.name} attr:value={initial} />
}

/** Radix's visually-hidden native <select> (inside a <form>). */
export function NativeSelect(props: { name: string; value: string | undefined; disabled: boolean; options: { label: string; value: string; disabled?: boolean }[]; onChange: (v: string) => void }): JSX.Element {
  let el: HTMLSelectElement | undefined
  const initial = untrack(() => props.value)
  createEffect(
    on(
      () => props.value,
      (v) => {
        if (!el) return
        Object.getOwnPropertyDescriptor(window.HTMLSelectElement.prototype, `value`)?.set?.call(el, v)
        el.dispatchEvent(new Event(`change`, { bubbles: true }))
      },
      { defer: true }
    )
  )
  const hidden = css({ position: `absolute`, border: 0, width: 1, height: 1, padding: 0, margin: -1, overflow: `hidden`, clip: `rect(0, 0, 0, 0)`, whiteSpace: `nowrap`, wordWrap: `normal` })
  return (
    <select ref={el} aria-hidden="true" tabindex={-1} name={props.name} disabled={props.disabled} style={hidden} onChange={(e) => props.onChange(e.currentTarget.value)}>
      {initial === undefined ? <option value="" /> : null}
      <For each={props.options}>
        {(o) => (
          <option value={o.value} disabled={bool(o.disabled)} attr:selected={initial !== undefined && o.value === initial ? `` : undefined}>
            {str(o.label)}
          </option>
        )}
      </For>
    </select>
  )
}
