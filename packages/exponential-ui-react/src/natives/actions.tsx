// VAPP-87 + round 1: Button, Link, Toggle. A Button is OPTIMISTIC: it goes
// disabled (and `aria-busy`) the moment it is pressed and stays so until the
// host's `onAction` promise settles. A `submit` Button submits the enclosing
// Form (and shows loading while the Form is `busy`).
//
// EXP-1259 (feedback-states.json `pending`): loading refuses presses at
// once but SHOWS only after `PENDING_DELAY_MS` (a fast action never
// flashes): then the icon slot is the spinner and the label `pendingLabel`,
// at no less than the idle width.

import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent, type MouseEvent, type PointerEvent } from "react"
import { PENDING_DELAY_MS } from "@exponential_app/ui"
import { Toggle as TogglePrimitive } from "radix-ui"
import { useSurfaceContext } from "../context"
import { useForm } from "../form"
import { CHROME, IconGlyph } from "../icons"
import { useBoundState } from "./bound"
import { ButtonHold, linkHref } from "@exponential_app/ui/web"
import type { NativeProps } from "../node-view"
import { bool, str, useParts } from "./shared"

export function usePending() {
  const [pending, setPending] = useState(false)
  const mounted = useRef(true)
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])
  const track = (result: void | Promise<void>) => {
    if (result && typeof (result as Promise<void>).then === `function`) {
      setPending(true)
      ;(result as Promise<void>).then(
        () => mounted.current && setPending(false),
        () => mounted.current && setPending(false)
      )
    }
  }
  return { pending, track }
}

export function ButtonNative({ node, props, rootProps, emit, children }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const form = useForm()
  const submit = bool(props.submit) && form !== null
  const { pending, track } = usePending()
  const label = str(props.label)
  const icon = str(props.icon)
  const loading = bool(props.loading) || pending || (submit && form.busy)
  const disabled = bool(props.disabled) || loading
  const iconOnly = props.size === `icon`
  const Spinner = CHROME.spinner
  const hasChild = node.children.length > 0
  const shown = usePendingShown(loading)
  const pendingLabel = str(props.pendingLabel)
  const shownLabel = shown && pendingLabel ? pendingLabel : label
  // The idle width is the pending state's minimum.
  const ref = useRef<HTMLButtonElement | null>(null)
  const idleWidth = useRef<number | null>(null)
  // The width the pending label keeps: read while loading but not yet
  // shown (the idle label still painted), never on an idle render (a read
  // there forces a layout in every commit).
  useLayoutEffect(() => {
    if (loading && !shown && ref.current) idleWidth.current = ref.current.offsetWidth || null
  })
  const rootStyle = (rootProps as { style?: Record<string, unknown> }).style
  // VAPP-162 (round-5 contract §9): `hold` = holdStart/holdEnd/holdCancel, never `press`.
  const hold = bool(props.hold)
  const { held, handlers } = useHold(hold, disabled, emit)
  const holdingLabel = str(props.holdingLabel)
  const paintedLabel = held && holdingLabel ? holdingLabel : shownLabel
  const states = [...(ctx.states ?? []), disabled ? `disabled` : ``, pending ? `pending` : ``, held ? `pressed` : ``].filter(Boolean).join(` `)
  return (
    <button
      {...(rootProps as Record<string, unknown>)}
      ref={ref}
      style={shown && idleWidth.current ? { ...rootStyle, minWidth: idleWidth.current } : rootStyle}
      type="button"
      disabled={disabled}
      aria-busy={loading || undefined}
      data-pending={shown ? `true` : undefined}
      aria-label={iconOnly && label ? label : undefined}
      data-xs={states || undefined}
      data-submit={submit ? `true` : undefined}
      data-hold={hold ? `true` : undefined}
      {...(hold ? handlers : {})}
      onClick={(e: MouseEvent) => {
        e.stopPropagation()
        if (hold) return
        track(emit(`press`))
        if (submit) form.submit()
      }}
    >
      {shown ? (
        <span {...(part(`spinner`) as Record<string, string>)}>
          <Spinner aria-hidden="true" />
        </span>
      ) : icon ? (
        <span {...(part(`icon`) as Record<string, string>)}>
          <IconGlyph icons={ctx.host.icons} name={icon} />
        </span>
      ) : null}
      {hasChild ? children : !iconOnly && paintedLabel ? <span {...(part(`label`) as Record<string, string>)}>{paintedLabel}</span> : null}
    </button>
  )
}

/** THE hold machine (`@exponential_app/ui/web` ButtonHold) wired to a
 *  button's DOM events: the pointer is captured on down (a long-press or a
 *  scroll never steals it), the slop is measured against the button's box,
 *  Space/Enter hold from key down to key up, Escape and blur cancel. */
function useHold(enabled: boolean, disabled: boolean, emit: NativeProps[`emit`]) {
  const [held, setHeld] = useState(false)
  const emitRef = useRef(emit)
  emitRef.current = emit
  const disabledRef = useRef(disabled)
  disabledRef.current = disabled
  const machine = useRef<ButtonHold | null>(null)
  machine.current ??= new ButtonHold((event) => void emitRef.current(event), setHeld, () => disabledRef.current)
  useEffect(() => {
    if (disabled || !enabled) machine.current?.reset()
  }, [disabled, enabled])
  useEffect(() => () => machine.current?.reset(), [])
  const m = machine.current
  const rect = (el: HTMLElement) => el.getBoundingClientRect()
  const handlers = {
    onPointerDown: (e: PointerEvent<HTMLButtonElement>) => {
      if (e.button !== undefined && e.button !== 0) return
      if (!m.pointerDown(e.pointerId ?? null)) return
      try {
        e.currentTarget.setPointerCapture?.(e.pointerId)
      } catch {
        // A synthetic pointer (tests) has no capture.
      }
    },
    onPointerMove: (e: PointerEvent<HTMLButtonElement>) => m.pointerMove(rect(e.currentTarget), e.clientX ?? 0, e.clientY ?? 0, e.pointerId ?? null),
    onPointerUp: (e: PointerEvent<HTMLButtonElement>) => m.pointerUp(rect(e.currentTarget), e.clientX ?? 0, e.clientY ?? 0, e.pointerId ?? null),
    onPointerCancel: (e: PointerEvent<HTMLButtonElement>) => m.pointerCancel(e.pointerId ?? null),
    onKeyDown: (e: KeyboardEvent<HTMLButtonElement>) => {
      if (m.keyDown(e.key, e.repeat)) e.preventDefault()
    },
    onKeyUp: (e: KeyboardEvent<HTMLButtonElement>) => {
      if (m.keyUp(e.key)) e.preventDefault()
    },
    onBlur: () => m.blur(),
    onContextMenu: (e: MouseEvent) => e.preventDefault(),
  }
  return { held: enabled && held, handlers }
}

/** `loading` held for `PENDING_DELAY_MS` (false again at once when it ends). */
export function usePendingShown(loading: boolean): boolean {
  const [shown, setShown] = useState(false)
  useEffect(() => {
    if (!loading) {
      setShown(false)
      return
    }
    const t = setTimeout(() => setShown(true), PENDING_DELAY_MS)
    return () => clearTimeout(t)
  }, [loading])
  return loading && shown
}

export function LinkNative({ props, rootProps, emit }: NativeProps) {
  const ctx = useSurfaceContext()
  const raw = str(props.href)
  // VAPP-103: the href passes the URL policy; a denied one paints as text.
  const href = linkHref(ctx.host, raw)
  const external = bool(props.external)
  const label = str(props.label) || raw
  return (
    <a
      {...(rootProps as Record<string, unknown>)}
      href={href}
      data-denied={raw && !href ? `` : undefined}
      target={href && external ? `_blank` : undefined}
      rel={href && external ? `noreferrer noopener` : undefined}
      onClick={(e: MouseEvent) => {
        const handled = emit(`press`)
        if (!href) e.preventDefault()
        else if (!external && handled !== undefined) e.preventDefault()
        else if (ctx.host.openUrl) {
          // A host opener takes every allowed href, external ones too (a
          // new tab is only the fallback without one).
          e.preventDefault()
          ctx.openUrl(href)
        }
      }}
    >
      {label}
    </a>
  )
}

export function ToggleNative({ node, props, rootProps, emit, scope }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const [pressed, setPressed] = useBoundState(node, scope, `pressed`, bool(props.pressed))
  const icon = str(props.icon)
  const label = str(props.label)
  return (
    <TogglePrimitive.Root
      {...(rootProps as Record<string, unknown>)}
      pressed={pressed}
      onPressedChange={(next) => {
        setPressed(next)
        void emit(`change`, { pressed: next }, { pressed: next })
      }}
      aria-label={icon && label ? label : undefined}
    >
      {icon ? (
        <span {...(part(`icon`) as Record<string, string>)}>
          <IconGlyph icons={ctx.host.icons} name={icon} />
        </span>
      ) : null}
      {label ? <span {...(part(`label`) as Record<string, string>)}>{label}</span> : null}
    </TogglePrimitive.Root>
  )
}
