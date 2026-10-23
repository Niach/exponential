import { Show, createContext, createEffect, createSignal, on, onCleanup, splitProps, useContext, type Accessor, type ComponentProps, type JSX } from "solid-js"
import { AVATAR_CLASSES, avatarFallbackColors, avatarFallbackHue } from "@exponential_app/ui/web/primitives"

import { createImageLoadingStatus, type LoadingStatus } from "../behaviors"
import { css } from "../style"
import { cn } from "./cn"

// Radix Avatar on plain DOM: a <span> root holding the image-loading status,
// the <img> painted only once its src LOADED (a detached Image probes it),
// the fallback painted while it has not, after `delayMs` when given.

interface AvatarState {
  status: Accessor<LoadingStatus>
  setStatus: (s: LoadingStatus) => void
}
const AvatarContext = createContext<AvatarState>()

function Avatar(props: ComponentProps<`span`>) {
  const [local, rest] = splitProps(props, [`class`])
  const [status, setStatus] = createSignal<LoadingStatus>(`idle`)
  return (
    <AvatarContext.Provider value={{ status, setStatus }}>
      <span data-slot="avatar" class={cn(AVATAR_CLASSES.root, local.class)} {...rest} />
    </AvatarContext.Provider>
  )
}

function AvatarImage(props: ComponentProps<`img`> & { onLoadingStatusChange?: (status: LoadingStatus) => void }) {
  const ctx = useContext(AvatarContext)!
  const [local, rest] = splitProps(props, [`class`, `src`, `onLoadingStatusChange`])
  const status = createImageLoadingStatus(() => local.src)
  createEffect(
    on(status, (s) => {
      if (s === `idle`) return
      local.onLoadingStatusChange?.(s)
      ctx.setStatus(s)
    })
  )
  return (
    <Show when={status() === `loaded`}>
      <img data-slot="avatar-image" class={cn(AVATAR_CLASSES.image, local.class)} {...rest} src={local.src} />
    </Show>
  )
}

function AvatarFallback(props: Omit<ComponentProps<`span`>, `style`> & { userId?: string | null; delayMs?: number; style?: JSX.CSSProperties }) {
  const ctx = useContext(AvatarContext)!
  const [local, rest] = splitProps(props, [`class`, `userId`, `style`, `delayMs`])
  const [canRender, setCanRender] = createSignal(local.delayMs === undefined)
  createEffect(
    on(
      () => local.delayMs,
      (ms) => {
        if (ms === undefined) return
        const timer = window.setTimeout(() => setCanRender(true), ms)
        onCleanup(() => window.clearTimeout(timer))
      }
    )
  )
  const hue = () => avatarFallbackHue(local.userId)
  const style = () => {
    const h = hue()
    return h === undefined ? local.style : { ...css(avatarFallbackColors(h)), ...(local.style ?? {}) }
  }
  return (
    <Show when={canRender() && ctx.status() !== `loaded`}>
      <span data-slot="avatar-fallback" class={cn(AVATAR_CLASSES.fallback, hue() === undefined && AVATAR_CLASSES.fallbackMuted, local.class)} style={style()} {...rest} />
    </Show>
  )
}

export { Avatar, AvatarImage, AvatarFallback }
