// VAPP-87 + round 1 (contract §3 Select = the combobox; a11y.json Select).
// A plain single Select is Radix's (arrows, type-ahead, Home/End built in).
// `searchable` and/or `multiple` is the COMBOBOX: a trigger
// (role=combobox, aria-expanded/controls) opening a listbox; with
// `searchable` the filter field keeps focus and drives the active option
// through aria-activedescendant, typing fires `search {query}` (150 ms
// debounce). Options come from the `options` prop (bindable: the host
// refills the bound path on `search`, the async source), or from the host's
// `optionSource(source, query)` when `source` names a host list. Local
// filtering applies only to inline, unbound options. `emptyText` (default
// `$string.noResults`) shows when nothing matches.

import { useEffect, useId, useMemo, useRef, useState } from "react"
import { Popover as PopoverPrimitive, Select as SelectPrimitive } from "radix-ui"
import { OVERLAY_OFFSET, OVERLAY_PADDING } from "@exponential-at/ui"
import { useSurfaceContext } from "../context"
import { boundPath } from "../data"
import { DEBOUNCE_MS } from "../inputs"
import { FieldErrors, useField } from "../form"
import { IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import { useBoundState } from "./bound"
import { useListNavigation } from "./listbox"
import { bool, BuiltinIcon, str, useParts, TextPart, objects } from "./shared"

export interface SelectOption {
  label: string
  value: string
  icon?: string
  disabled?: boolean
}

const asList = (raw: unknown): string[] => (Array.isArray(raw) ? raw.map(String) : raw === undefined || raw === null || raw === `` ? [] : String(raw).split(`,`))

export function SelectNative({ node, props, rootProps, emit, scope, domId }: NativeProps) {
  const part = useParts(node, props)
  const ctx = useSurfaceContext()
  const id = useId()
  const listId = `${id}-list`
  const multiple = bool(props.multiple)
  const searchable = bool(props.searchable)
  const placeholder = str(props.placeholder) || ctx.t(`choose`)
  const raw = props.value
  const rawIsArray = Array.isArray(raw)
  const [value, setValue] = useBoundState<string | string[]>(node, scope, `value`, multiple ? asList(raw) : str(raw))
  const triggerRef = useRef<HTMLButtonElement | null>(null)
  const f = useField({ node, domId, props, value, focusRef: triggerRef })
  const disabled = bool(props.disabled) || Boolean(f.form?.disabled)
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState(``)
  const source = str(props.source)
  const optionsBound = boundPath(node.props, `options`, scope) !== undefined
  const [sourced, setSourced] = useState<SelectOption[] | null>(null)
  // Every optionSource request is numbered; a response that is not the
  // latest one is dropped (an older query resolving last must not win).
  const request = useRef(0)
  const askSource = (q: string) => {
    if (!source || !ctx.host.optionSource) return
    const mine = ++request.current
    const r = ctx.host.optionSource(source, q)
    const take = (list: SelectOption[] | null | undefined) => {
      if (mine === request.current) setSourced(objects<SelectOption>(list))
    }
    if (r && typeof (r as Promise<SelectOption[]>).then === `function`) void (r as Promise<SelectOption[]>).then(take, () => undefined)
    else take(r as SelectOption[])
  }
  useEffect(
    () => () => {
      request.current++
    },
    []
  )
  const inline = objects<SelectOption>(props.options)
  const options = sourced ?? inline
  const commit = (next: string | string[]) => {
    const out = multiple && !rawIsArray && typeof raw === `string` ? (next as string[]).join(`,`) : next
    setValue(next)
    f.change(out)
    f.touch()
    void emit(`change`, { value: out })
  }

  // `search {query}` debounced; a host source answers the same query.
  useEffect(() => {
    if (!searchable || !open) return
    const t = setTimeout(() => {
      void emit(`search`, { query })
      askSource(query)
    }, DEBOUNCE_MS)
    return () => clearTimeout(t)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, open, searchable, source])
  useEffect(() => {
    if (!source || !ctx.host.optionSource || searchable) return
    askSource(``)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [source, ctx.host, searchable])

  const label = <TextPart part={part(`label`)} text={props.label} as="label" htmlFor={id} />
  const triggerStates = (o: boolean) => [o && `open`, disabled && `disabled`, f.invalid && `invalid`] as const

  if (!multiple && !searchable) {
    const chosen = options.find((o) => o.value === value)
    return (
      <div {...(rootProps as Record<string, unknown>)} data-source={source || undefined}>
        {label}
        <SelectPrimitive.Root value={(value as string) || undefined} onValueChange={commit} disabled={disabled} open={open} onOpenChange={setOpen} name={str(props.name, node.id)} dir={ctx.direction}>
          <SelectPrimitive.Trigger ref={triggerRef} id={id} {...(part(`trigger`, ...triggerStates(open)) as Record<string, string>)} aria-invalid={f.invalid || undefined} aria-describedby={f.describedBy}>
            <span>{chosen ? <SelectPrimitive.Value>{str(chosen.label)}</SelectPrimitive.Value> : <span {...(part(`placeholder`) as Record<string, string>)}>{placeholder}</span>}</span>
            <SelectPrimitive.Icon asChild>
              <BuiltinIcon slot="Select.trigger" />
            </SelectPrimitive.Icon>
          </SelectPrimitive.Trigger>
          <SelectPrimitive.Portal container={ctx.portal ?? undefined}>
            <SelectPrimitive.Content {...(part(`content`, open && `open`) as Record<string, string>)} position="popper" sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING} data-xui-overlay="Select" style={{ minWidth: `var(--radix-select-trigger-width)` }}>
              <SelectPrimitive.Viewport>
                {options.length === 0 ? <div {...(part(`empty`) as Record<string, string>)}>{str(props.emptyText) || ctx.t(`noResults`)}</div> : null}
                {options.map((o) => (
                  <SelectPrimitive.Item key={o.value} value={o.value} disabled={bool(o.disabled)} {...(part(`item`, o.value === value && `selected`, bool(o.disabled) && `disabled`) as Record<string, string>)}>
                    {o.icon ? <IconGlyph icons={ctx.host.icons} name={o.icon} /> : null}
                    <SelectPrimitive.ItemText>{str(o.label)}</SelectPrimitive.ItemText>
                    <SelectPrimitive.ItemIndicator {...(part(`check`) as Record<string, string>)} className={`${part(`check`).className as string} xui-Select-check`}>
                      <BuiltinIcon slot="Select.check" size={16} />
                    </SelectPrimitive.ItemIndicator>
                  </SelectPrimitive.Item>
                ))}
              </SelectPrimitive.Viewport>
            </SelectPrimitive.Content>
          </SelectPrimitive.Portal>
        </SelectPrimitive.Root>
        <FieldErrors part={part} field={f} />
      </div>
    )
  }

  return (
    <Combobox
      {...{ node, props, rootProps, part, id, listId, multiple, searchable, placeholder, options, value, commit, open, setOpen, query, setQuery, disabled, triggerRef, label, source }}
      localFilter={!optionsBound && !source}
      field={f}
    />
  )
}

interface ComboboxProps {
  node: NativeProps[`node`]
  props: Record<string, unknown>
  rootProps: Record<string, unknown>
  part: ReturnType<typeof useParts>
  id: string
  listId: string
  multiple: boolean
  searchable: boolean
  placeholder: string
  options: SelectOption[]
  value: string | string[]
  commit: (next: string | string[]) => void
  open: boolean
  setOpen: (open: boolean) => void
  query: string
  setQuery: (q: string) => void
  disabled: boolean
  triggerRef: { current: HTMLButtonElement | null }
  label: React.ReactNode
  source: string
  localFilter: boolean
  field: ReturnType<typeof useField>
}

function Combobox({ props, rootProps, part, id, listId, multiple, searchable, placeholder, options, value, commit, open, setOpen, query, setQuery, disabled, triggerRef, label, source, localFilter, field }: ComboboxProps) {
  const ctx = useSurfaceContext()
  const selected = multiple ? (value as string[]) : value ? [value as string] : []
  const chosen = options.filter((o) => selected.includes(o.value))
  const shown = useMemo(() => (query && localFilter ? options.filter((o) => str(o.label).toLowerCase().includes(query.toLowerCase())) : options), [options, query, localFilter])
  const listRef = useRef<HTMLDivElement | null>(null)
  const pick = (i: number) => {
    const o = shown[i]
    if (!o || bool(o.disabled)) return
    if (multiple) commit(selected.includes(o.value) ? selected.filter((v) => v !== o.value) : [...selected, o.value])
    else {
      commit(o.value)
      setOpen(false)
    }
  }
  const nav = useListNavigation({ count: shown.length, isDisabled: (i) => bool(shown[i]?.disabled), labelOf: (i) => str(shown[i]?.label), onPick: pick, typeahead: !searchable })
  useEffect(() => {
    if (open) {
      const i = shown.findIndex((o) => selected.includes(o.value))
      nav.setActive(i >= 0 ? i : shown.findIndex((o) => !bool(o.disabled)))
    } else setQuery(``)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])
  useEffect(() => {
    if (!searchable || !open) return
    nav.setActive(shown.findIndex((o) => !bool(o.disabled)))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query])
  const optionId = (i: number) => `${listId}-${i}`
  const activeId = nav.active >= 0 && nav.active < shown.length ? optionId(nav.active) : undefined
  useEffect(() => {
    if (activeId) listRef.current?.querySelector(`[id="${activeId}"]`)?.scrollIntoView?.({ block: `nearest` })
  }, [activeId])
  return (
    <div {...(rootProps as Record<string, unknown>)} data-source={source || undefined}>
      {label}
      <PopoverPrimitive.Root open={open} onOpenChange={setOpen}>
        <PopoverPrimitive.Trigger
          ref={triggerRef}
          id={id}
          {...(part(`trigger`, open && `open`, disabled && `disabled`, field.invalid && `invalid`) as Record<string, string>)}
          disabled={disabled}
          role="combobox"
          aria-haspopup="listbox"
          aria-expanded={open}
          aria-controls={open ? listId : undefined}
          aria-invalid={field.invalid || undefined}
          aria-describedby={field.describedBy}
          onKeyDown={(e) => {
            if (!open && (e.key === `ArrowDown` || e.key === `ArrowUp`)) {
              e.preventDefault()
              setOpen(true)
            }
          }}
        >
          <span>{chosen.length ? chosen.map((o) => str(o.label)).join(`, `) : <span {...(part(`placeholder`) as Record<string, string>)}>{placeholder}</span>}</span>
          <BuiltinIcon slot="Select.trigger" />
        </PopoverPrimitive.Trigger>
        <PopoverPrimitive.Portal container={ctx.portal ?? undefined}>
          <PopoverPrimitive.Content
            {...(part(`content`, open && `open`) as Record<string, string>)}
            dir={ctx.direction}
            align="start"
            sideOffset={OVERLAY_OFFSET}
            collisionPadding={OVERLAY_PADDING}
            data-xui-overlay="Select"
            style={{ minWidth: `var(--radix-popover-trigger-width)` }}
            onOpenAutoFocus={(e) => {
              if (!searchable) {
                e.preventDefault()
                listRef.current?.focus()
              }
            }}
          >
            {searchable ? (
              <div className="xui-Select-searchbox">
                <BuiltinIcon slot="Select.search" size={16} />
                <input
                  {...(part(`search`) as Record<string, string>)}
                  className={`${part(`search`).className as string} xui-Select-search`}
                  role="combobox"
                  aria-expanded
                  aria-controls={listId}
                  aria-activedescendant={activeId}
                  aria-autocomplete="list"
                  placeholder={ctx.t(`search`)}
                  aria-label={ctx.t(`search`)}
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  onKeyDown={nav.onKeyDown}
                />
              </div>
            ) : null}
            <div
              ref={listRef}
              id={listId}
              role="listbox"
              tabIndex={searchable ? -1 : 0}
              aria-multiselectable={multiple || undefined}
              aria-activedescendant={searchable ? undefined : activeId}
              aria-labelledby={id}
              className="xui-Select-listbox"
              onKeyDown={searchable ? undefined : nav.onKeyDown}
            >
              {shown.length === 0 ? <div {...(part(`empty`) as Record<string, string>)}>{str(props.emptyText) || ctx.t(`noResults`)}</div> : null}
              {shown.map((o, i) => {
                const isSel = selected.includes(o.value)
                const isActive = i === nav.active
                return (
                  <div
                    key={o.value}
                    id={optionId(i)}
                    role="option"
                    aria-selected={isSel}
                    aria-disabled={bool(o.disabled) || undefined}
                    data-active={isActive ? `` : undefined}
                    {...(part(`item`, (isSel || isActive) && `selected`, bool(o.disabled) && `disabled`, isActive && `hover`) as Record<string, string>)}
                    onPointerMove={() => nav.setActive(i)}
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => pick(i)}
                  >
                    {o.icon ? <IconGlyph icons={ctx.host.icons} name={o.icon} /> : null}
                    <span>{str(o.label)}</span>
                    {isSel ? (
                      <span {...(part(`check`) as Record<string, string>)} className={`${part(`check`).className as string} xui-Select-check`}>
                        <BuiltinIcon slot="Select.check" size={16} />
                      </span>
                    ) : null}
                  </div>
                )
              })}
            </div>
          </PopoverPrimitive.Content>
        </PopoverPrimitive.Portal>
      </PopoverPrimitive.Root>
      <FieldErrors part={part} field={field} />
    </div>
  )
}
