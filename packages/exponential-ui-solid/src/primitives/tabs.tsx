import { Show, createContext, createSignal, createUniqueId, onCleanup, onMount, splitProps, untrack, useContext, type Accessor, type ComponentProps, type JSX } from "solid-js"
import { TABS_CLASSES } from "@exponential_app/ui/web/primitives"

import { createPresence, rovingFocusFirst, focusIntent, wrapArray } from "../behaviors"
import { cn } from "./cn"

export { SEGMENTED_ITEM, SEGMENTED_LIST, SEGMENTED_ROW, SEGMENTED_ROW_COMPACT, SEGMENTED_TAB } from "@exponential_app/ui/web/primitives"

// Radix Tabs on plain DOM: the root (`dir`, `data-orientation`), the list =
// a RovingFocusGroup (its own tab stop until a trigger holds it, entry focus
// to the active trigger, arrows/Home/End by orientation and `dir`, `loop`),
// the triggers (`role="tab"`, ids `<base>-trigger-<value>` /
// `<base>-content-<value>`, mouse down / Space / Enter / focus select), the
// panels (`hidden` + no children while inactive; the panel active at mount
// keeps `animation-duration: 0s` until the first render after its frame).

type Orientation = `horizontal` | `vertical`
type Dir = `ltr` | `rtl`

interface TabsState {
  baseId: string
  value: Accessor<string>
  setValue: (v: string) => void
  orientation: Accessor<Orientation>
  dir: Accessor<Dir>
  activationMode: Accessor<`automatic` | `manual`>
}
const TabsContext = createContext<TabsState>()

interface RovingItem {
  el: HTMLElement
  focusable: Accessor<boolean>
  active: Accessor<boolean>
}
interface RovingState {
  orientation: Accessor<Orientation>
  dir: Accessor<Dir>
  loop: Accessor<boolean>
  stop: Accessor<HTMLElement | null>
  setStop: (el: HTMLElement | null) => void
  shiftTab: () => void
  register: (item: RovingItem) => void
  items: () => RovingItem[]
}
const RovingContext = createContext<RovingState>()

type HandlerProp = unknown
const call = (handler: HandlerProp, e: Event) => {
  if (typeof handler === `function`) handler(e)
  else if (Array.isArray(handler)) handler[0](handler[1], e)
}

export type TabsProps = Omit<ComponentProps<`div`>, `dir`> & {
  value?: string
  defaultValue?: string
  onValueChange?: (value: string) => void
  orientation?: Orientation
  dir?: Dir
  activationMode?: `automatic` | `manual`
}

function Tabs(props: TabsProps) {
  const [local, rest] = splitProps(props, [`class`, `value`, `defaultValue`, `onValueChange`, `orientation`, `dir`, `activationMode`])
  const [inner, setInner] = createSignal(untrack(() => local.defaultValue) ?? ``)
  const controlled = () => local.value !== undefined
  const value = () => (controlled() ? (local.value as string) : inner())
  const setValue = (next: string) => {
    const prev = untrack(value)
    if (!untrack(controlled)) setInner(next)
    if (next !== prev) local.onValueChange?.(next)
  }
  const state: TabsState = {
    baseId: createUniqueId(),
    value,
    setValue,
    orientation: () => local.orientation ?? `horizontal`,
    dir: () => local.dir ?? `ltr`,
    activationMode: () => local.activationMode ?? `automatic`,
  }
  return (
    <TabsContext.Provider value={state}>
      <div dir={state.dir()} data-orientation={state.orientation()} data-slot="tabs" class={cn(TABS_CLASSES.root, local.class)} {...rest} />
    </TabsContext.Provider>
  )
}

export type TabsListProps = ComponentProps<`div`> & { loop?: boolean }

function TabsList(props: TabsListProps) {
  const ctx = useContext(TabsContext)!
  const [local, rest] = splitProps(props, [`class`, `loop`, `style`, `onMouseDown`])
  const [stop, setStop] = createSignal<HTMLElement | null>(null)
  const [tabbingBackOut, setTabbingBackOut] = createSignal(false)
  const [registered, setRegistered] = createSignal<RovingItem[]>([])
  let clickFocus = false
  const items = () => {
    const all = registered().filter((i) => i.el.isConnected)
    return all.sort((a, b) => (a.el.compareDocumentPosition(b.el) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1))
  }
  const roving: RovingState = {
    orientation: ctx.orientation,
    dir: ctx.dir,
    loop: () => local.loop ?? true,
    stop,
    setStop,
    shiftTab: () => setTabbingBackOut(true),
    register: (item) => {
      setRegistered((list) => [...list, item])
      onCleanup(() => setRegistered((list) => list.filter((i) => i !== item)))
    },
    items,
  }
  const focusableCount = () => registered().filter((i) => i.focusable()).length
  return (
    <RovingContext.Provider value={roving}>
      <div
        role="tablist"
        aria-orientation={ctx.orientation()}
        tabindex={tabbingBackOut() || focusableCount() === 0 ? -1 : 0}
        data-orientation={ctx.orientation()}
        data-slot="tabs-list"
        class={cn(TABS_CLASSES.list, local.class)}
        {...rest}
        style={{ outline: `none`, ...((local.style as JSX.CSSProperties | undefined) ?? {}) }}
        onMouseDown={(e) => {
          call(local.onMouseDown, e)
          if (!e.defaultPrevented) clickFocus = true
        }}
        // React's onFocus/onBlur bubble: Radix's group handlers see the
        // triggers' focus moves too.
        onFocusIn={(e) => {
          const keyboard = !clickFocus
          if (e.target === e.currentTarget && keyboard && !untrack(tabbingBackOut)) {
            const candidates = items().filter((i) => i.focusable())
            const active = candidates.find((i) => i.active())
            const current = candidates.find((i) => i.el === untrack(stop))
            rovingFocusFirst([active, current, ...candidates].filter((i): i is RovingItem => Boolean(i)).map((i) => i.el))
          }
          clickFocus = false
        }}
        onFocusOut={() => setTabbingBackOut(false)}
      />
    </RovingContext.Provider>
  )
}

export type TabsTriggerProps = Omit<ComponentProps<`button`>, `value`> & { value: string; disabled?: boolean }

function TabsTrigger(props: TabsTriggerProps) {
  const ctx = useContext(TabsContext)!
  const roving = useContext(RovingContext)!
  const [local, rest] = splitProps(props, [`class`, `value`, `disabled`, `onMouseDown`, `onKeyDown`, `onFocus`, `ref`])
  const disabled = () => local.disabled ?? false
  const selected = () => local.value === ctx.value()
  let el!: HTMLButtonElement
  onMount(() => roving.register({ el, focusable: () => !disabled(), active: selected }))
  return (
    <button
      type="button"
      role="tab"
      aria-selected={selected()}
      aria-controls={`${ctx.baseId}-content-${local.value}`}
      data-state={selected() ? `active` : `inactive`}
      data-disabled={disabled() ? `` : undefined}
      disabled={disabled()}
      id={`${ctx.baseId}-trigger-${local.value}`}
      data-slot="tabs-trigger"
      class={cn(TABS_CLASSES.trigger, local.class)}
      {...rest}
      tabindex={roving.stop() !== null && roving.stop() === el ? 0 : -1}
      data-orientation={roving.orientation()}
      data-radix-collection-item=""
      ref={(node) => {
        el = node
        if (typeof local.ref === `function`) (local.ref as (el: HTMLButtonElement) => void)(node)
      }}
      onMouseDown={(e) => {
        call(local.onMouseDown, e)
        if (e.defaultPrevented) return
        if (!disabled() && e.button === 0 && e.ctrlKey === false) ctx.setValue(local.value)
        else e.preventDefault()
        if (e.defaultPrevented) return
        roving.setStop(el)
      }}
      onKeyDown={(e) => {
        call(local.onKeyDown, e)
        if (!e.defaultPrevented && [` `, `Enter`].includes(e.key)) ctx.setValue(local.value)
        if (e.defaultPrevented) return
        if (e.key === `Tab` && e.shiftKey) {
          roving.shiftTab()
          return
        }
        if (e.target !== e.currentTarget) return
        const intent = focusIntent(e, roving.orientation(), roving.dir())
        if (intent === undefined) return
        if (e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return
        e.preventDefault()
        let candidates = roving.items().filter((i) => i.focusable()).map((i) => i.el)
        if (intent === `last`) candidates.reverse()
        else if (intent === `prev` || intent === `next`) {
          if (intent === `prev`) candidates.reverse()
          const at = candidates.indexOf(e.currentTarget as HTMLElement)
          candidates = roving.loop() ? wrapArray(candidates, at + 1) : candidates.slice(at + 1)
        }
        setTimeout(() => rovingFocusFirst(candidates))
      }}
      onFocus={(e) => {
        call(local.onFocus, e)
        if (!e.defaultPrevented && !selected() && !disabled() && ctx.activationMode() !== `manual`) ctx.setValue(local.value)
        if (!e.defaultPrevented) roving.setStop(el)
      }}
    />
  )
}

export type TabsContentProps = Omit<ComponentProps<`div`>, `style`> & { value: string; forceMount?: boolean; style?: JSX.CSSProperties }

function TabsContent(props: TabsContentProps) {
  const ctx = useContext(TabsContext)!
  const [local, rest] = splitProps(props, [`class`, `value`, `forceMount`, `children`, `style`])
  const selected = () => local.value === ctx.value()
  const presence = createPresence(() => Boolean(local.forceMount) || selected())
  // Radix reads its mount-animation flag at render time: the panel selected
  // at mount keeps `0s` until the first render after its frame (a value change).
  let mountPrevented = untrack(selected)
  onMount(() => {
    const raf = requestAnimationFrame(() => (mountPrevented = false))
    onCleanup(() => cancelAnimationFrame(raf))
  })
  const style = (): JSX.CSSProperties | undefined => {
    ctx.value()
    const own = local.style
    if (mountPrevented) return { ...(own ?? {}), "animation-duration": `0s` }
    return own
  }
  return (
    <div
      data-state={selected() ? `active` : `inactive`}
      data-orientation={ctx.orientation()}
      role="tabpanel"
      aria-labelledby={`${ctx.baseId}-trigger-${local.value}`}
      hidden={!presence.mounted()}
      id={`${ctx.baseId}-content-${local.value}`}
      tabindex={0}
      data-slot="tabs-content"
      class={cn(TABS_CLASSES.content, local.class)}
      {...rest}
      ref={presence.ref}
      style={style()}
    >
      <Show when={presence.mounted()}>{local.children}</Show>
    </div>
  )
}

export { Tabs, TabsList, TabsTrigger, TabsContent }
