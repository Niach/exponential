// VAPP-87 + round 1 + round 3: Tabs, Segmented (was ToggleGroup), Accordion — Radix behaviour (roving
// tab stop, arrows wrap, Home/End: a11y.json), recipe paint, a bound
// `value` written through (multiple = comma-joined when it came as a
// string, an array when it came as one).

import { Fragment, useRef, useState, type ReactNode } from "react"
import { useBoundState } from "./bound"
import { Accordion as AccordionPrimitive, Tabs as TabsPrimitive, ToggleGroup as ToggleGroupPrimitive } from "radix-ui"
import { useSurfaceContext } from "../context"
import { IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import { NodeView } from "../node-view"
import { bool, BuiltinIcon, num, str, useParts, type PartFn, objects } from "./shared"
import { segmentedBadge } from "@exponential_app/ui/web"

interface Tab {
  label: string
  value: string
  icon?: string
  count?: number
}

export function TabsNative({ node, props, rootProps, emit, scope }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const tabs = objects<Tab>(props.tabs)
  const external = str(props.value) || tabs[0]?.value || ``
  const [value, setValue] = useBoundState(node, scope, `value`, external)
  const fill = bool(props.fill)
  // EXP-1259: the close glyph per tab (`closable`; `hover` = only while its
  // tab or itself is hovered or keyboard-focused: the tab LENDS it those
  // states) and the + after the last tab (`addable`).
  const closable = props.closable === `hover` || props.closable === `always`
  const [lent, setLent] = useState<{ index: number; state: `hover` | `focus` } | null>(null)
  const lend = (index: number, state: `hover` | `focus` | null) => setLent((cur) => (state ? { index, state } : cur?.index === index ? null : cur))
  const lending = closable && props.closable === `hover`
  return (
    <TabsPrimitive.Root
      {...(rootProps as Record<string, unknown>)}
      value={value}
      onValueChange={(next) => {
        setValue(next)
        void emit(`change`, { value: next }, { value: next })
      }}
      orientation="horizontal"
      activationMode="automatic"
      dir={ctx.direction}
    >
      <TabsPrimitive.List {...(part(`list`) as Record<string, string>)} data-fill={fill ? `true` : undefined} loop>
        {tabs.map((tab, i) => {
          const selected = tab.value === value
          const trigger = (
            <TabsPrimitive.Trigger
              key={tab.value}
              value={tab.value}
              {...(part.at(`tab`, i, selected && `selected`) as Record<string, string>)}
              onPointerEnter={lending ? () => lend(i, `hover`) : undefined}
              onPointerLeave={lending ? () => lend(i, null) : undefined}
              onFocus={lending ? (e) => (e.currentTarget.matches(`:focus-visible`) ? lend(i, `focus`) : undefined) : undefined}
              onBlur={lending ? () => lend(i, null) : undefined}
            >
              <span className="xui-Tabs-tab-body">
                {tab.icon ? <IconGlyph icons={ctx.host.icons} name={tab.icon} className="xui-icon" width={16} height={16} /> : null}
                <span>{str(tab.label)}</span>
                {tab.count !== undefined ? <span data-count="">{ctx.formatter.number(num(tab.count))}</span> : null}
              </span>
              <span {...(part(`indicator`, selected && `selected`) as Record<string, string>)} aria-hidden="true" />
            </TabsPrimitive.Trigger>
          )
          if (!closable) return trigger
          return (
            <Fragment key={tab.value}>
              {trigger}
              <button
                type="button"
                {...(part.at(`close`, i, lent?.index === i && lent.state) as Record<string, string>)}
                aria-label={ctx.t(`closeTab`, { name: str(tab.label) })}
                onPointerEnter={lending ? () => lend(i, `hover`) : undefined}
                onPointerLeave={lending ? () => lend(i, null) : undefined}
                onClick={() => void emit(`close`, { value: tab.value })}
              >
                <BuiltinIcon slot="Tabs.close" />
              </button>
            </Fragment>
          )
        })}
        {bool(props.addable) ? (
          <button type="button" {...(part(`new`) as Record<string, string>)} aria-label={ctx.t(`newTab`)} onClick={() => void emit(`new`)}>
            <BuiltinIcon slot="Tabs.new" />
          </button>
        ) : null}
      </TabsPrimitive.List>
      {tabs.map((tab, i) => {
        // Only the ACTIVE panel is placed (`<id>.content`); the others are hidden.
        const attrs = part(`content`) as Record<string, string>
        if (tab.value !== value) delete attrs[`data-xui-id`]
        return (
          <TabsPrimitive.Content key={tab.value} value={tab.value} {...attrs}>
            {node.children[i] ? <NodeView node={node.children[i]} /> : null}
          </TabsPrimitive.Content>
        )
      })}
    </TabsPrimitive.Root>
  )
}

interface Item {
  label: string
  value: string
  icon?: string
  disabled?: boolean
  /** VAPP-162: a `bar` item's count badge. */
  count?: number
}

const joinLike = (raw: unknown, next: string[]): string | string[] => (Array.isArray(raw) ? next : next.join(`,`))

/** Segmented (round 3): ONE row of segments.
 *  `segmented` (default, the pill track) / `toggles` (bare) / `outline`
 *  (joined) = a Radix ToggleGroup (radiogroup single, toolbar of toggle
 *  buttons multiple; roving tab stop, arrows wrap, Home/End). `bar` = the
 *  full-width bottom destinations: role navigation, each item a column
 *  button (icon above a caption label), the selected one
 *  `aria-current="page"`, the same roving keys. Parts: root, item, icon,
 *  label (the selected state rides every part of the selected item).
 *  VAPP-162 (round-5 contract §3): `orientation: vertical` stacks every
 *  variant top to bottom (ArrowUp/ArrowDown move focus); a vertical `bar`
 *  is THE navigation rail (72 px items). A `bar` item's `count` paints the
 *  `badge` at its icon's top-end corner (`99+` above 99, none at 0) and
 *  reads after the label (`Chats, 3`). */
export function SegmentedNative({ node, props, rootProps, emit, scope }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const items = objects<Item>(props.items)
  const variant = str(props.variant, `segmented`)
  const bar = variant === `bar`
  const vertical = props.orientation === `vertical`
  const multiple = !bar && props.type === `multiple`
  const externalRaw = props.value
  const external = multiple ? (Array.isArray(externalRaw) ? externalRaw.map(String) : externalRaw ? String(externalRaw).split(`,`) : []) : str(externalRaw)
  const [value, setBound] = useBoundState<string | string[]>(node, scope, `value`, external)
  const setValue = (next: string | string[]) => setBound(Array.isArray(next) ? (joinLike(externalRaw, next) as string[]) : next)
  const isSelected = (item: Item) => (multiple ? asArray(value).includes(item.value) : value === item.value)
  const content = (item: Item, selected: boolean) => {
    const badge = bar ? segmentedBadge(item.count) : null
    return (
    <>
      {item.icon ? (
        <span {...(part(`icon`, selected && `selected`) as Record<string, string>)} aria-hidden="true">
          <IconGlyph icons={ctx.host.icons} name={item.icon} className="xui-icon" width={bar ? 20 : 16} height={bar ? 20 : 16} />
          {badge ? <span {...(part(`badge`, selected && `selected`) as Record<string, string>)}>{badge}</span> : null}
        </span>
      ) : null}
      {item.label ? <span {...(part(`label`, selected && `selected`) as Record<string, string>)}>{str(item.label)}</span> : null}
    </>
    )
  }
  if (bar) return <SegmentedBar node={node} rootProps={rootProps} vertical={vertical} items={items} value={value as string} part={part} content={content} onPick={(next) => {
    setValue(next)
    void emit(`change`, { value: next }, { value: next })
  }} />
  const body = items.map((item) => {
    const selected = isSelected(item)
    return (
      <ToggleGroupPrimitive.Item key={item.value} value={item.value} disabled={bool(item.disabled)} aria-label={item.label ? undefined : item.value} {...(part(`item`, selected && `selected`, bool(item.disabled) && `disabled`) as Record<string, string>)}>
        {content(item, selected)}
      </ToggleGroupPrimitive.Item>
    )
  })
  if (multiple) {
    return (
      <ToggleGroupPrimitive.Root
        {...(rootProps as Record<string, unknown>)}
        type="multiple"
        value={Array.isArray(value) ? value : asArray(value)}
        loop
        orientation={vertical ? `vertical` : undefined}
        onValueChange={(next: string[]) => {
          setValue(next)
          void emit(`change`, { value: joinLike(externalRaw, next) }, { value: joinLike(externalRaw, next) })
        }}
        dir={ctx.direction}
      >
        {body}
      </ToggleGroupPrimitive.Root>
    )
  }
  return (
    <ToggleGroupPrimitive.Root
      {...(rootProps as Record<string, unknown>)}
      type="single"
      value={value as string}
      loop
      orientation={vertical ? `vertical` : undefined}
      onValueChange={(next: string) => {
        if (!next) return
        setValue(next)
        void emit(`change`, { value: next }, { value: next })
      }}
      dir={ctx.direction}
    >
      {body}
    </ToggleGroupPrimitive.Root>
  )
}

/** The `bar` variant: a navigation landmark of buttons with ONE roving tab
 *  stop (the selected item, else the first enabled one). */
function SegmentedBar({ rootProps, vertical, items, value, part, content, onPick }: { node: NativeProps[`node`]; rootProps: NativeProps[`rootProps`]; vertical: boolean; items: Item[]; value: string; part: PartFn; content: (item: Item, selected: boolean) => ReactNode; onPick: (value: string) => void }) {
  const ctx = useSurfaceContext()
  const refs = useRef<(HTMLButtonElement | null)[]>([])
  const enabled = items.map((item, i) => (bool(item.disabled) ? -1 : i)).filter((i) => i >= 0)
  const selectedIndex = items.findIndex((item) => item.value === value && !bool(item.disabled))
  const [focusIndex, setFocusIndex] = useState<number | null>(null)
  const stop = focusIndex ?? (selectedIndex >= 0 ? selectedIndex : (enabled[0] ?? -1))
  const move = (from: number, key: string) => {
    if (!enabled.length) return
    const at = enabled.indexOf(from)
    const rtl = ctx.direction === `rtl`
    const forward = key === `ArrowDown` || (!vertical && key === (rtl ? `ArrowLeft` : `ArrowRight`))
    const back = key === `ArrowUp` || (!vertical && key === (rtl ? `ArrowRight` : `ArrowLeft`))
    let next: number | null = null
    if (forward) next = enabled[(at + 1) % enabled.length]
    else if (back) next = enabled[(at - 1 + enabled.length) % enabled.length]
    else if (key === `Home`) next = enabled[0]
    else if (key === `End`) next = enabled[enabled.length - 1]
    if (next === null || next === undefined) return false
    setFocusIndex(next)
    refs.current[next]?.focus()
    return true
  }
  return (
    <nav {...(rootProps as Record<string, unknown>)} dir={ctx.direction}>
      {items.map((item, i) => {
        const selected = item.value === value
        const disabled = bool(item.disabled)
        return (
          <button
            key={item.value}
            ref={(el) => void (refs.current[i] = el)}
            type="button"
            {...(part(`item`, selected && `selected`, disabled && `disabled`) as Record<string, string>)}
            disabled={disabled}
            tabIndex={i === stop ? 0 : -1}
            aria-current={selected ? `page` : undefined}
            aria-label={segmentedBadge(item.count) ? `${str(item.label) || item.value}, ${segmentedBadge(item.count)}` : item.label ? undefined : item.value}
            onFocus={() => setFocusIndex(i)}
            onKeyDown={(e) => {
              if (move(i, e.key)) e.preventDefault()
            }}
            onClick={() => onPick(item.value)}
          >
            {content(item, selected)}
          </button>
        )
      })}
    </nav>
  )
}

interface AccordionItem {
  title: string
  value: string
  count?: number
}

export function AccordionNative({ node, props, rootProps, emit, scope }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const items = objects<AccordionItem>(props.items)
  const multiple = props.type === `multiple`
  const raw = props.value
  const external = multiple ? (Array.isArray(raw) ? raw.map(String) : raw ? String(raw).split(`,`) : []) : str(raw)
  const [value, setBound] = useBoundState<string | string[]>(node, scope, `value`, external)
  const setValue = (next: string | string[]) => setBound(Array.isArray(next) ? (joinLike(raw, next) as string[]) : next)
  const body = items.map((item, i) => {
    const open = multiple ? asArray(value).includes(item.value) : value === item.value
    return (
      <AccordionPrimitive.Item key={item.value} value={item.value} {...(part.at(`item`, i, open && `open`) as Record<string, string>)}>
        <AccordionPrimitive.Header asChild>
          <div style={{ display: `flex` }}>
            <AccordionPrimitive.Trigger {...(part.at(`trigger`, i, open && `open`) as Record<string, string>)}>
              <span className="xui-accordion-title">
                {str(item.title)}
                {/* Round 2 §7: the count is its own muted part after the title. */}
                {item.count !== undefined ? <span {...(part.at(`count`, i) as Record<string, string>)}>{ctx.formatter.number(num(item.count))}</span> : null}
              </span>
              <BuiltinIcon slot="Accordion.trigger" size={16} />
            </AccordionPrimitive.Trigger>
          </div>
        </AccordionPrimitive.Header>
        <AccordionPrimitive.Content {...(part.at(`content`, i, open && `open`) as Record<string, string>)}>{node.children[i] ? <NodeView node={node.children[i]} /> : null}</AccordionPrimitive.Content>
      </AccordionPrimitive.Item>
    )
  })
  if (multiple) {
    return (
      <AccordionPrimitive.Root
        {...(rootProps as Record<string, unknown>)}
        type="multiple"
        value={asArray(value)}
        dir={ctx.direction}
        onValueChange={(next: string[]) => {
          setValue(next)
          void emit(`change`, { value: joinLike(raw, next) }, { value: joinLike(raw, next) })
        }}
      >
        {body}
      </AccordionPrimitive.Root>
    )
  }
  return (
    <AccordionPrimitive.Root
      {...(rootProps as Record<string, unknown>)}
      type="single"
      collapsible
      dir={ctx.direction}
      value={value as string}
      onValueChange={(next: string) => {
        setValue(next)
        void emit(`change`, { value: next }, { value: next })
      }}
    >
      {body}
    </AccordionPrimitive.Root>
  )
}

const asArray = (v: string | string[]): string[] => (Array.isArray(v) ? v : v ? v.split(`,`) : [])
