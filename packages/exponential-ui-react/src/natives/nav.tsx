// VAPP-87: Tabs, ToggleGroup, Accordion — Radix behaviour, recipe paint.

import { useEffect, useState } from "react"
import { Accordion as AccordionPrimitive, Tabs as TabsPrimitive, ToggleGroup as ToggleGroupPrimitive } from "radix-ui"
import { useSurfaceContext } from "../context"
import { CHROME, IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import { NodeView } from "../node-view"
import { arr, bool, num, str, useParts } from "./shared"

interface Tab {
  label: string
  value: string
  icon?: string
  count?: number
}

export function TabsNative({ node, props, rootProps, emit }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const tabs = arr<Tab>(props.tabs)
  const external = str(props.value) || tabs[0]?.value || ``
  const [value, setValue] = useState(external)
  useEffect(() => setValue(external), [external])
  const fill = bool(props.fill)
  return (
    <TabsPrimitive.Root
      {...(rootProps as Record<string, unknown>)}
      value={value}
      onValueChange={(next) => {
        setValue(next)
        void emit(`change`, { value: next })
      }}
      orientation="horizontal"
      dir={ctx.direction}
    >
      <TabsPrimitive.List {...(part(`list`) as Record<string, string>)} data-fill={fill ? `true` : undefined}>
        {tabs.map((tab) => {
          const selected = tab.value === value
          return (
            <TabsPrimitive.Trigger key={tab.value} value={tab.value} {...(part(`tab`, selected && `selected`) as Record<string, string>)}>
              <span className="xui-Tabs-tab-body">
                {tab.icon ? <IconGlyph icons={ctx.host.icons} name={tab.icon} className="xui-icon" width={16} height={16} /> : null}
                <span>{str(tab.label)}</span>
                {tab.count !== undefined ? <span data-count="">{num(tab.count)}</span> : null}
              </span>
              <span {...(part(`indicator`, selected && `selected`) as Record<string, string>)} aria-hidden="true" />
            </TabsPrimitive.Trigger>
          )
        })}
      </TabsPrimitive.List>
      {tabs.map((tab, i) => (
        <TabsPrimitive.Content key={tab.value} value={tab.value} {...(part(`content`) as Record<string, string>)}>
          {node.children[i] ? <NodeView node={node.children[i]} /> : null}
        </TabsPrimitive.Content>
      ))}
    </TabsPrimitive.Root>
  )
}

interface Item {
  label: string
  value: string
  icon?: string
  disabled?: boolean
}

export function ToggleGroupNative({ node, props, rootProps, emit }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const items = arr<Item>(props.items)
  const multiple = props.type === `multiple`
  const externalRaw = props.value
  const external = multiple ? (Array.isArray(externalRaw) ? externalRaw.map(String) : externalRaw ? String(externalRaw).split(`,`) : []) : str(externalRaw)
  const [value, setValue] = useState<string | string[]>(external)
  useEffect(() => setValue(external), [JSON.stringify(external)])
  const body = items.map((item) => {
    const selected = multiple ? (value as string[]).includes(item.value) : value === item.value
    return (
      <ToggleGroupPrimitive.Item key={item.value} value={item.value} disabled={bool(item.disabled)} {...(part(`item`, selected && `selected`, bool(item.disabled) && `disabled`) as Record<string, string>)} aria-label={item.icon && !item.label ? item.label : undefined}>
        {item.icon ? <IconGlyph icons={ctx.host.icons} name={item.icon} className="xui-icon" width={16} height={16} /> : null}
        {item.label ? <span>{str(item.label)}</span> : null}
      </ToggleGroupPrimitive.Item>
    )
  })
  if (multiple) {
    return (
      <ToggleGroupPrimitive.Root
        {...(rootProps as Record<string, unknown>)}
        type="multiple"
        value={value as string[]}
        onValueChange={(next: string[]) => {
          setValue(next)
          void emit(`change`, { value: next })
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
      onValueChange={(next: string) => {
        if (!next) return
        setValue(next)
        void emit(`change`, { value: next })
      }}
      dir={ctx.direction}
    >
      {body}
    </ToggleGroupPrimitive.Root>
  )
}

interface AccordionItem {
  title: string
  value: string
  count?: number
}

export function AccordionNative({ node, props, rootProps, emit }: NativeProps) {
  const part = useParts(node, props)
  const items = arr<AccordionItem>(props.items)
  const multiple = props.type === `multiple`
  const raw = props.value
  const external = multiple ? (Array.isArray(raw) ? raw.map(String) : raw ? String(raw).split(`,`) : []) : str(raw)
  const [value, setValue] = useState<string | string[]>(external)
  useEffect(() => setValue(external), [JSON.stringify(external)])
  const Chevron = CHROME.chevronDown
  const body = items.map((item, i) => {
    const open = multiple ? (value as string[]).includes(item.value) : value === item.value
    return (
      <AccordionPrimitive.Item key={item.value} value={item.value} {...(part(`item`, open && `open`) as Record<string, string>)}>
        <AccordionPrimitive.Header asChild>
          <div style={{ display: `flex` }}>
            <AccordionPrimitive.Trigger {...(part(`trigger`, open && `open`) as Record<string, string>)}>
              <span>
                {str(item.title)}
                {item.count !== undefined ? ` · ${num(item.count)}` : ``}
              </span>
              <Chevron aria-hidden="true" width={16} height={16} />
            </AccordionPrimitive.Trigger>
          </div>
        </AccordionPrimitive.Header>
        <AccordionPrimitive.Content {...(part(`content`, open && `open`) as Record<string, string>)}>{node.children[i] ? <NodeView node={node.children[i]} /> : null}</AccordionPrimitive.Content>
      </AccordionPrimitive.Item>
    )
  })
  if (multiple) {
    return (
      <AccordionPrimitive.Root
        {...(rootProps as Record<string, unknown>)}
        type="multiple"
        value={value as string[]}
        onValueChange={(next: string[]) => {
          setValue(next)
          void emit(`change`, { value: next })
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
      value={value as string}
      onValueChange={(next: string) => {
        setValue(next)
        void emit(`change`, { value: next })
      }}
    >
      {body}
    </AccordionPrimitive.Root>
  )
}
