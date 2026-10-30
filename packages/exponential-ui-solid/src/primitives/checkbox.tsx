import { Show, createEffect, createSignal, onCleanup, splitProps, untrack, type ComponentProps, type JSX } from "solid-js"
import { Check } from "lucide-solid"
import { CHECKBOX_CLASSES } from "@exponential_app/eui/web/primitives"

import { bubbleOnChange, createElementSize, createInForm, createPresence } from "../behaviors"
import { PaintIcon } from "../icons"
import { css } from "../style"
import { cn } from "./cn"

// Radix Checkbox on plain DOM: the button (`role="checkbox"`, `aria-checked`
// true|false|"mixed", `data-state`, `value`), the indicator only while
// checked or indeterminate (Radix Presence), and inside a <form> the hidden
// bubble input Radix renders after the button.

export type CheckedState = boolean | `indeterminate`

export type CheckboxProps = Omit<ComponentProps<`button`>, `value` | `onChange`> & {
  checked?: CheckedState
  defaultChecked?: CheckedState
  onCheckedChange?: (checked: CheckedState) => void
  required?: boolean
  name?: string
  value?: string
  form?: string
  disabled?: boolean
}

const isIndeterminate = (c: CheckedState | undefined): c is `indeterminate` => c === `indeterminate`
const stateOf = (c: CheckedState) => (isIndeterminate(c) ? `indeterminate` : c ? `checked` : `unchecked`)

function Checkbox(props: CheckboxProps) {
  const [local, rest] = splitProps(props, [`class`, `checked`, `defaultChecked`, `onCheckedChange`, `required`, `name`, `value`, `form`, `disabled`, `onKeyDown`, `onClick`, `ref`])
  const [inner, setInner] = createSignal<CheckedState>(untrack(() => local.defaultChecked) ?? false)
  const controlled = () => local.checked !== undefined
  const checked = (): CheckedState => (controlled() ? (local.checked as CheckedState) : inner())
  const setChecked = (next: CheckedState) => {
    const prev = untrack(checked)
    if (!untrack(controlled)) setInner(() => next)
    if (next !== prev) local.onCheckedChange?.(next)
  }
  const [control, setControl] = createSignal<HTMLButtonElement>()
  const inForm = createInForm(control)
  const value = () => local.value ?? `on`
  const initial = untrack(checked)
  let stopped = false
  createEffect(() => {
    const form = control()?.form
    if (!form) return
    const reset = () => setChecked(initial)
    form.addEventListener(`reset`, reset)
    onCleanup(() => form.removeEventListener(`reset`, reset))
  })
  const presence = createPresence(() => isIndeterminate(checked()) || checked() === true)
  const call = <E extends Event>(handler: unknown, e: E) => {
    if (typeof handler === `function`) handler(e)
    else if (Array.isArray(handler)) handler[0](handler[1], e)
  }
  return (
    <>
      <button
        type="button"
        role="checkbox"
        aria-checked={isIndeterminate(checked()) ? `mixed` : (checked() as boolean)}
        aria-required={local.required}
        data-state={stateOf(checked())}
        data-disabled={local.disabled ? `` : undefined}
        disabled={local.disabled}
        value={value()}
        data-slot="checkbox"
        class={cn(CHECKBOX_CLASSES.root, local.class)}
        {...rest}
        ref={(el) => {
          setControl(el)
          if (typeof local.ref === `function`) (local.ref as (el: HTMLButtonElement) => void)(el)
        }}
        onKeyDown={(e) => {
          call(local.onKeyDown, e)
          if (e.defaultPrevented) return
          if (e.key === `Enter`) e.preventDefault()
        }}
        onClick={(e) => {
          call(local.onClick, e)
          if (e.defaultPrevented) return
          setChecked(isIndeterminate(untrack(checked)) ? true : !untrack(checked))
          if (inForm()) {
            stopped = e.cancelBubble
            if (!stopped) e.stopPropagation()
          }
        }}
      >
        <Show when={presence.mounted()}>
          <span ref={presence.ref} data-state={stateOf(checked())} data-disabled={local.disabled ? `` : undefined} data-slot="checkbox-indicator" class={CHECKBOX_CLASSES.indicator} style={css({ pointerEvents: `none` })}>
            <PaintIcon icon={Check} class={CHECKBOX_CLASSES.icon} />
          </span>
        </Show>
      </button>
      <Show when={inForm()}>
        <CheckboxBubble control={control()} checked={checked()} initial={initial} required={local.required} disabled={local.disabled} name={local.name} value={value()} form={local.form} bubbles={() => !stopped} />
      </Show>
    </>
  )
}

/** Radix's `CheckboxBubbleInput`: `defaultChecked` = the checked state at
 *  mount (indeterminate = false), the control's size appended once measured. */
function CheckboxBubble(props: { control: HTMLButtonElement | undefined; checked: CheckedState; initial: CheckedState; required?: boolean; disabled?: boolean; name?: string; value: string; form?: string; bubbles: () => boolean }): JSX.Element {
  let input: HTMLInputElement | undefined
  const size = createElementSize(() => props.control)
  bubbleOnChange(
    () => input,
    () => props.checked,
    (el, v) => {
      ;(el as HTMLInputElement).indeterminate = isIndeterminate(v)
      Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, `checked`)?.set?.call(el, isIndeterminate(v) ? false : v)
    },
    `click`,
    props.bubbles
  )
  const style = () => {
    const s = size()
    return css({ ...(s ? { width: s.width, height: s.height } : {}), position: `absolute`, pointerEvents: `none`, opacity: 0, margin: 0, transform: `translateX(-100%)` })
  }
  return <input ref={input} type="checkbox" aria-hidden="true" attr:checked={props.initial === true ? `` : undefined} required={props.required} disabled={props.disabled} name={props.name} attr:value={props.value} form={props.form} tabindex={-1} style={style()} />
}

export { Checkbox }
