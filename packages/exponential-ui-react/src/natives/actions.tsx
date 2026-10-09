// VAPP-87 + round 1: Button, Link, Toggle. A Button is OPTIMISTIC: it goes
// disabled (and `aria-busy`) the moment it is pressed and stays so until the
// host's `onAction` promise settles. A `submit` Button submits the enclosing
// Form (and shows loading while the Form is `busy`).

import { useEffect, useRef, useState, type MouseEvent } from "react"
import { Toggle as TogglePrimitive } from "radix-ui"
import { useSurfaceContext } from "../context"
import { useForm } from "../form"
import { CHROME, IconGlyph } from "../icons"
import { useBoundState } from "./bound"
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
  return (
    <button
      {...(rootProps as Record<string, unknown>)}
      type="button"
      disabled={disabled}
      aria-busy={loading || undefined}
      aria-label={iconOnly && label ? label : undefined}
      data-xs={[...(ctx.states ?? []), disabled ? `disabled` : ``, pending ? `pending` : ``].filter(Boolean).join(` `) || undefined}
      data-submit={submit ? `true` : undefined}
      onClick={(e: MouseEvent) => {
        e.stopPropagation()
        track(emit(`press`))
        if (submit) form.submit()
      }}
    >
      {loading ? (
        <span {...(part(`spinner`) as Record<string, string>)}>
          <Spinner aria-hidden="true" />
        </span>
      ) : icon ? (
        <span {...(part(`icon`) as Record<string, string>)}>
          <IconGlyph icons={ctx.host.icons} name={icon} />
        </span>
      ) : null}
      {hasChild ? children : !iconOnly && label ? <span {...(part(`label`) as Record<string, string>)}>{label}</span> : null}
    </button>
  )
}

export function LinkNative({ props, rootProps, emit }: NativeProps) {
  const ctx = useSurfaceContext()
  const href = str(props.href)
  const external = bool(props.external)
  const label = str(props.label) || href
  return (
    <a
      {...(rootProps as Record<string, unknown>)}
      href={href || `#`}
      target={external ? `_blank` : undefined}
      rel={external ? `noreferrer noopener` : undefined}
      onClick={(e: MouseEvent) => {
        const handled = emit(`press`)
        if (!href) e.preventDefault()
        else if (!external && handled !== undefined) e.preventDefault()
        else if (href && ctx.host.openUrl && !external) {
          e.preventDefault()
          ctx.host.openUrl(href)
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
        void emit(`change`, { pressed: next })
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
