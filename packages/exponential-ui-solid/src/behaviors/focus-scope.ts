// FEED-131: Radix `FocusScope`: the scope stack (a newer scope pauses the
// one below), auto focus on mount and back on unmount, the trap, the Tab loop.

import { onCleanup, untrack, type Accessor } from "solid-js"

interface Scope {
  paused: boolean
}
let SCOPES: Scope[] = []
const scopesAdd = (s: Scope) => {
  const active = SCOPES[0]
  if (active && active !== s) active.paused = true
  SCOPES = [s, ...SCOPES.filter((x) => x !== s)]
}
const scopesRemove = (s: Scope) => {
  SCOPES = SCOPES.filter((x) => x !== s)
  if (SCOPES[0]) SCOPES[0].paused = false
}

export function tabbables(container: HTMLElement): HTMLElement[] {
  const nodes: HTMLElement[] = []
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_ELEMENT, {
    acceptNode: (node) => {
      const el = node as HTMLElement & { disabled?: boolean; type?: string }
      const hiddenInput = el.tagName === `INPUT` && el.type === `hidden`
      if (el.disabled || el.hidden || hiddenInput) return NodeFilter.FILTER_SKIP
      return el.tabIndex >= 0 ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP
    },
  })
  while (walker.nextNode()) nodes.push(walker.currentNode as HTMLElement)
  return nodes
}
export function isHidden(node: HTMLElement | null, upTo: HTMLElement): boolean {
  if (node && getComputedStyle(node).visibility === `hidden`) return true
  while (node) {
    if (node === upTo) return false
    if (getComputedStyle(node).display === `none`) return true
    node = node.parentElement
  }
  return false
}
export function focusEl(el: HTMLElement | null | undefined, select = false) {
  if (!el?.focus) return
  const prev = document.activeElement
  el.focus({ preventScroll: true })
  if (el !== prev && select && el instanceof HTMLInputElement && `select` in el) el.select()
}
export function focusFirst(candidates: (HTMLElement | null | undefined)[], select = false) {
  const prev = document.activeElement
  for (const c of candidates) {
    focusEl(c, select)
    if (document.activeElement !== prev) return
  }
}

export const AUTOFOCUS_ON_MOUNT = `focusScope.autoFocusOnMount`
export const AUTOFOCUS_ON_UNMOUNT = `focusScope.autoFocusOnUnmount`

/** Radix `FocusScope` on `el`: auto focus on mount (the first tabbable, else
 *  the container; `onMountAutoFocus` may prevent it), focus back on unmount
 *  (`onUnmountAutoFocus` may prevent it), `trapped` keeps the focus inside,
 *  `loop` wraps Tab. Returns the container's keydown handler. */
export function createFocusScope(o: { trapped: Accessor<boolean>; loop: boolean; onMountAutoFocus?: (e: Event) => void; onUnmountAutoFocus?: (e: Event) => void }): { attach: (el: HTMLElement) => void; onKeyDown: (e: KeyboardEvent) => void } {
  const scope: Scope = { paused: false }
  let container: HTMLElement | undefined
  const attach = (el: HTMLElement) => {
    // Radix's FocusScope effect runs one commit AFTER the dismissable layers
    // of the same mount registered their outside listeners (its container
    // is state): every overlay mounted together listens before any of them
    // takes the focus (a Popover open next to an opening Dialog loses).
    let started = false
    let disposed = false
    let previouslyFocused: HTMLElement | null = null
    let lastFocused: HTMLElement | null = null
    const focusIn = (event: FocusEvent) => {
      if (!untrack(o.trapped) || scope.paused) return
      const target = event.target as HTMLElement | null
      if (el.contains(target)) lastFocused = target
      else focusEl(lastFocused, true)
    }
    const focusOut = (event: FocusEvent) => {
      if (!untrack(o.trapped) || scope.paused) return
      const related = event.relatedTarget as HTMLElement | null
      if (related === null) return
      if (!el.contains(related)) focusEl(lastFocused, true)
    }
    const observer = new MutationObserver((mutations) => {
      if (!untrack(o.trapped) || scope.paused) return
      if (document.activeElement !== document.body) return
      for (const m of mutations) if (m.removedNodes.length > 0) focusEl(el)
    })
    queueMicrotask(() => {
      if (disposed || !el.isConnected) return
      started = true
      container = el
      scopesAdd(scope)
      previouslyFocused = document.activeElement as HTMLElement | null
      if (!el.contains(previouslyFocused)) {
        const mountEvent = new CustomEvent(AUTOFOCUS_ON_MOUNT, { bubbles: false, cancelable: true })
        const onMount = (e: Event) => o.onMountAutoFocus?.(e)
        el.addEventListener(AUTOFOCUS_ON_MOUNT, onMount)
        el.dispatchEvent(mountEvent)
        el.removeEventListener(AUTOFOCUS_ON_MOUNT, onMount)
        if (!mountEvent.defaultPrevented) {
          focusFirst(tabbables(el).filter((n) => n.tagName !== `A`), true)
          if (document.activeElement === previouslyFocused) focusEl(el)
        }
      }
      // The trap (Radix: focusin/focusout on the document + a mutation watch).
      document.addEventListener(`focusin`, focusIn)
      document.addEventListener(`focusout`, focusOut)
      observer.observe(el, { childList: true, subtree: true })
    })
    onCleanup(() => {
      disposed = true
      if (!started) return
      document.removeEventListener(`focusin`, focusIn)
      document.removeEventListener(`focusout`, focusOut)
      observer.disconnect()
      const prev = previouslyFocused
      setTimeout(() => {
        const unmountEvent = new CustomEvent(AUTOFOCUS_ON_UNMOUNT, { bubbles: false, cancelable: true })
        const onUnmount = (e: Event) => o.onUnmountAutoFocus?.(e)
        el.addEventListener(AUTOFOCUS_ON_UNMOUNT, onUnmount)
        el.dispatchEvent(unmountEvent)
        if (!unmountEvent.defaultPrevented) focusEl(prev ?? document.body, true)
        el.removeEventListener(AUTOFOCUS_ON_UNMOUNT, onUnmount)
        scopesRemove(scope)
      }, 0)
    })
  }
  const onKeyDown = (event: KeyboardEvent) => {
    if (!o.loop && !untrack(o.trapped)) return
    if (scope.paused || !container) return
    const isTab = event.key === `Tab` && !event.altKey && !event.ctrlKey && !event.metaKey
    const focused = document.activeElement as HTMLElement | null
    if (!isTab || !focused) return
    const candidates = tabbables(container)
    const first = candidates.find((c) => !isHidden(c, container!))
    const last = [...candidates].reverse().find((c) => !isHidden(c, container!))
    if (!first || !last) {
      if (focused === container) event.preventDefault()
    } else if (!event.shiftKey && focused === last) {
      event.preventDefault()
      if (o.loop) focusEl(first, true)
    } else if (event.shiftKey && focused === first) {
      event.preventDefault()
      if (o.loop) focusEl(last, true)
    }
  }
  return { attach, onKeyDown }
}
