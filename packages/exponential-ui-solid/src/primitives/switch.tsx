import { Show, createEffect, createSignal, splitProps, untrack, type ComponentProps, type JSX } from "solid-js"
import { SWITCH_CLASSES } from "@exponential_app/eui/web/primitives"

import { bubbleOnChange, createElementSize, createInForm, setChecked as setInputChecked } from "../behaviors"
import { css } from "../style"
import { cn } from "./cn"

// Radix Switch on plain DOM: the button (`role="switch"`, `aria-checked`,
// `data-state`, `value`) holding the thumb, and inside a <form> the hidden
// bubble input whose `checked` attribute follows the state (React writes
// `defaultChecked` on every render).

export type SwitchProps = Omit<ComponentProps<`button`>, `value` | `onChange`> & {
  checked?: boolean
  defaultChecked?: boolean
  onCheckedChange?: (checked: boolean) => void
  required?: boolean
  name?: string
  value?: string
  form?: string
  disabled?: boolean
  size?: `sm` | `default`
}

function Switch(props: SwitchProps) {
  const [local, rest] = splitProps(props, [`class`, `size`, `checked`, `defaultChecked`, `onCheckedChange`, `required`, `name`, `value`, `form`, `disabled`, `onClick`, `ref`])
  const [inner, setInner] = createSignal(untrack(() => local.defaultChecked) ?? false)
  const controlled = () => local.checked !== undefined
  const checked = () => (controlled() ? Boolean(local.checked) : inner())
  const setChecked = (next: boolean) => {
    const prev = untrack(checked)
    if (!untrack(controlled)) setInner(next)
    if (next !== prev) local.onCheckedChange?.(next)
  }
  const [button, setButton] = createSignal<HTMLButtonElement>()
  const inForm = createInForm(button)
  const value = () => local.value ?? `on`
  const state = () => (checked() ? `checked` : `unchecked`)
  let stopped = false
  return (
    <>
      <button
        type="button"
        role="switch"
        aria-checked={checked()}
        aria-required={local.required}
        data-state={state()}
        data-disabled={local.disabled ? `` : undefined}
        disabled={local.disabled}
        value={value()}
        data-slot="switch"
        data-size={local.size ?? `default`}
        class={cn(SWITCH_CLASSES.root, local.class)}
        {...rest}
        ref={(el) => {
          setButton(el)
          if (typeof local.ref === `function`) (local.ref as (el: HTMLButtonElement) => void)(el)
        }}
        onClick={(e) => {
          const handler = local.onClick as unknown
          if (typeof handler === `function`) handler(e)
          else if (Array.isArray(handler)) handler[0](handler[1], e)
          if (e.defaultPrevented) return
          setChecked(!untrack(checked))
          if (inForm()) {
            stopped = e.cancelBubble
            if (!stopped) e.stopPropagation()
          }
        }}
      >
        <span data-state={state()} data-disabled={local.disabled ? `` : undefined} data-slot="switch-thumb" class={cn(SWITCH_CLASSES.thumb)} />
      </button>
      <Show when={inForm()}>
        <SwitchBubble control={button()} checked={checked()} required={local.required} disabled={local.disabled} name={local.name} value={value()} form={local.form} bubbles={() => !stopped} />
      </Show>
    </>
  )
}

/** Radix's `SwitchBubbleInput`: `transform` first (its own style prop),
 *  the control's size appended once measured. */
function SwitchBubble(props: { control: HTMLButtonElement | undefined; checked: boolean; required?: boolean; disabled?: boolean; name?: string; value: string; form?: string; bubbles: () => boolean }): JSX.Element {
  let input: HTMLInputElement | undefined
  const size = createElementSize(() => props.control)
  // React's `defaultChecked` update writes the attribute, never the property.
  createEffect(() => {
    if (input) input.defaultChecked = props.checked
  })
  bubbleOnChange(() => input, () => props.checked, setInputChecked, `click`, props.bubbles)
  const style = () => {
    const s = size()
    return css({ transform: `translateX(-100%)`, ...(s ? { width: s.width, height: s.height } : {}), position: `absolute`, pointerEvents: `none`, opacity: 0, margin: 0 })
  }
  return <input ref={input} type="checkbox" aria-hidden="true" attr:checked={untrack(() => props.checked) ? `` : undefined} required={props.required} disabled={props.disabled} name={props.name} attr:value={props.value} form={props.form} tabindex={-1} style={style()} />
}

export { Switch }
