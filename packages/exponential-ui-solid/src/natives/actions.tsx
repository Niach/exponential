// FEED-128 (React's natives/actions.tsx): Button, Link, Toggle. A Button is
// OPTIMISTIC: it goes disabled (and `aria-busy`) the moment it is pressed
// and stays so until the host's `onAction` promise settles. A `submit`
// Button submits the enclosing Form (and shows loading while the Form is
// `busy`).
//
// EXP-1259 (feedback-states.json `pending`): loading refuses presses at
// once but SHOWS only after `PENDING_DELAY_MS` (a fast action never
// flashes): then the icon slot is the spinner and the label `pendingLabel`,
// at no less than the idle width.
//
// Toggle paints Radix's Toggle DOM itself (`aria-pressed`, `data-state=
// "on|off"`): Kobalte's ToggleButton writes `data-pressed` instead.

import { Show, createEffect, createMemo, createSignal, onCleanup, type Accessor, type Component, type JSX } from "solid-js"
import { PENDING_DELAY_MS } from "@exponential-at/ui"
import { linkHref } from "@exponential-at/ui/web"
import { useSurfaceContext } from "../context"
import { useForm } from "../form"
import { CHROME, IconGlyph, PaintIcon } from "../icons"
import type { NativeProps } from "../node-view"
import { createBoundState } from "./bound"
import { bool, css, parts, str } from "./shared"

/** React's `usePending`: `pending` while the last tracked promise runs. */
export function createPending(): { pending: Accessor<boolean>; track: (result: void | Promise<void>) => void } {
  const [pending, setPending] = createSignal(false)
  let mounted = true
  onCleanup(() => (mounted = false))
  const track = (result: void | Promise<void>) => {
    if (result && typeof (result as Promise<void>).then === `function`) {
      setPending(true)
      ;(result as Promise<void>).then(
        () => mounted && setPending(false),
        () => mounted && setPending(false)
      )
    }
  }
  return { pending, track }
}

/** `loading` held for `PENDING_DELAY_MS` (false again at once when it ends). */
export function createPendingShown(loading: Accessor<boolean>): Accessor<boolean> {
  const [shown, setShown] = createSignal(false)
  createEffect(() => {
    if (!loading()) {
      setShown(false)
      return
    }
    const t = setTimeout(() => setShown(true), PENDING_DELAY_MS)
    onCleanup(() => clearTimeout(t))
  })
  return () => loading() && shown()
}

export function ButtonNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const form = useForm()
  const submit = () => bool(props.props.submit) && form !== null
  const { pending, track } = createPending()
  const label = () => str(props.props.label)
  const icon = () => str(props.props.icon)
  const loading = () => bool(props.props.loading) || pending() || (submit() && form!.busy)
  const disabled = () => bool(props.props.disabled) || loading()
  const iconOnly = () => props.props.size === `icon`
  const hasChild = () => props.node.children.length > 0
  const shown = createPendingShown(loading)
  const pendingLabel = () => str(props.props.pendingLabel)
  const shownLabel = () => (shown() && pendingLabel() ? pendingLabel() : label())
  // The idle width is the pending state's minimum: measured as the spinner
  // is about to show (the memo runs before the DOM updates, so it still
  // paints the idle content; React measures it after every idle render).
  let ref: HTMLButtonElement | undefined
  const idleWidth = createMemo<number | null>(() => (shown() ? (ref?.offsetWidth ?? 0) || null : null))
  // React leaves an emptied `style=""` once it wrote one.
  let styled = false
  const style = () => {
    const root = props.rootProps.style as Record<string, string> | undefined
    const w = shown() ? idleWidth() : null
    const out = w ? { ...(root ?? {}), ...css({ minWidth: w }) } : root
    if (out) styled = true
    return out ?? (styled ? {} : undefined)
  }
  const xs = () => [...(ctx.states ?? []), disabled() ? `disabled` : ``, pending() ? `pending` : ``].filter(Boolean).join(` `) || undefined
  return (
    <button
      {...props.rootProps}
      ref={ref}
      style={style()}
      type="button"
      disabled={disabled()}
      aria-busy={loading() || undefined}
      data-pending={shown() ? `true` : undefined}
      aria-label={iconOnly() && label() ? label() : undefined}
      data-xs={xs()}
      data-submit={submit() ? `true` : undefined}
      onClick={(e) => {
        e.stopPropagation()
        track(props.emit(`press`))
        if (submit()) form!.submit()
      }}
    >
      {shown() ? (
        <span {...part(`spinner`)}>
          <PaintIcon icon={CHROME.spinner} aria-hidden="true" />
        </span>
      ) : icon() ? (
        <span {...part(`icon`)}>
          <IconGlyph icons={ctx.host.icons} name={icon()} />
        </span>
      ) : null}
      <Show when={hasChild()} fallback={!iconOnly() && shownLabel() ? <span {...part(`label`)}>{shownLabel()}</span> : null}>
        {props.children}
      </Show>
    </button>
  )
}

export function LinkNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const raw = () => str(props.props.href)
  // VAPP-103: the href passes the URL policy; a denied one paints as text.
  const href = () => linkHref(ctx.host, raw())
  const external = () => bool(props.props.external)
  const label = () => str(props.props.label) || raw()
  return (
    <a
      {...props.rootProps}
      href={href()}
      data-denied={raw() && !href() ? `` : undefined}
      target={href() && external() ? `_blank` : undefined}
      rel={href() && external() ? `noreferrer noopener` : undefined}
      onClick={(e) => {
        const handled = props.emit(`press`)
        const h = href()
        if (!h) e.preventDefault()
        else if (!external() && handled !== undefined) e.preventDefault()
        else if (ctx.host.openUrl) {
          // A host opener takes every allowed href, external ones too (a
          // new tab is only the fallback without one).
          e.preventDefault()
          ctx.openUrl(h)
        }
      }}
    >
      {label()}
    </a>
  )
}

export function ToggleNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const [pressed, setPressed] = createBoundState(() => props.node, () => props.scope, `pressed`, () => bool(props.props.pressed))
  const icon = () => str(props.props.icon)
  const label = () => str(props.props.label)
  return (
    <button
      type="button"
      aria-pressed={pressed()}
      data-state={pressed() ? `on` : `off`}
      {...props.rootProps}
      aria-label={icon() && label() ? label() : undefined}
      onClick={() => {
        const next = !pressed()
        setPressed(next)
        void props.emit(`change`, { pressed: next }, { pressed: next })
      }}
    >
      {icon() ? (
        <span {...part(`icon`)}>
          <IconGlyph icons={ctx.host.icons} name={icon()} />
        </span>
      ) : null}
      {label() ? <span {...part(`label`)}>{label()}</span> : null}
    </button>
  )
}

export const NATIVES_ACTIONS: Record<string, Component<NativeProps>> = {
  Button: ButtonNative,
  Link: LinkNative,
  Toggle: ToggleNative,
}
