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
// `$string.noResults`) shows when nothing matches. #EXP-1629: a `row`
// `triggerVariant` is a Group row of parts (the label, a spacer, the
// `value` capped at `selectRowValueMaxWidth`, the `Select.chevron` glyph)
// whose popup hangs off the value (web/listbox.ts `selectPopupGeometry`);
// every popup opens at the top with the cursor on the first enabled option
// (the search field focused when searchable) and fades its list's bottom
// while options sit below; an item's `hover` = the cursor, `selected` =
// chosen.

import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react"
import { Popover as PopoverPrimitive, Select as SelectPrimitive } from "radix-ui"
import { useSurfaceContext } from "../context"
import { boundPath, firstEnabledIndex, listMoreBelow, revealOption, selectPopupGeometry, valueAnchorOffsets } from "@exponential_app/eui/web"
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

// VAPP-146 (fixtures/interactions-round1.json `select`): `""` is an ordinary
// option value. Radix's Select.Item refuses it (Radix keeps `""` for "no
// value"), so an empty value crosses the Radix boundary as EMPTY and comes
// back as `""`; the one DOM place Radix writes an item value (the hidden
// native <select> inside a <form>) is set back to `""`, so the DOM stays
// the Solid painter's.
const EMPTY = `\u0000xui-empty`
const toRadix = (value: string): string => (value === `` ? EMPTY : value)
const fromRadix = (value: string): string => (value === EMPTY ? `` : value)

/** Keeps the hidden native <select>'s options free of `EMPTY`; a select
 *  mounted while the value is `""` gets the empty placeholder option first
 *  and keeps it (the Solid painter's `NativeSelect`; Radix only renders it
 *  for an uncontrolled value). */
function useNativeEmptyOption(root: HTMLElement | null, empty: boolean): void {
  const emptyNow = useRef(empty)
  emptyNow.current = empty
  useEffect(() => {
    if (!root || typeof MutationObserver === `undefined`) return
    const seen = new WeakSet<HTMLSelectElement>()
    const fix = () => {
      for (const select of Array.from(root.querySelectorAll<HTMLSelectElement>(`select[aria-hidden]`))) {
        if (!seen.has(select)) {
          seen.add(select)
          if (emptyNow.current) select.insertBefore(document.createElement(`option`), select.firstChild).setAttribute(`value`, ``)
        }
        for (const option of Array.from(select.options)) if (option.getAttribute(`value`) === EMPTY) option.setAttribute(`value`, ``)
      }
    }
    fix()
    const observer = new MutationObserver(fix)
    observer.observe(root, { subtree: true, childList: true, attributes: true, attributeFilter: [`value`] })
    return () => observer.disconnect()
  }, [root])
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
  const [cursor, setCursor] = useState<string | null>(null)
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
  const hasEmpty = options.some((o) => o.value === ``)
  const [plainRoot, setPlainRoot] = useState<HTMLDivElement | null>(null)
  useNativeEmptyOption(hasEmpty ? plainRoot : null, value === ``)
  const commit = (next: string | string[]) => {
    const out = multiple && !rawIsArray && typeof raw === `string` ? (next as string[]).join(`,`) : next
    setValue(next, out)
    f.change(out)
    f.touch()
    void emit(`change`, { value: out }, { value: out })
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

  // #EXP-1629: a `row` trigger carries its label inside, leading.
  const row = str(props.triggerVariant) === `row`
  const label = row ? null : <TextPart part={part(`label`)} text={props.label} as="label" htmlFor={id} />
  const rowLabel = row ? <TextPart part={part(`label`)} text={props.label} /> : null
  const spacer = row ? <span className="xui-Select-spacer" aria-hidden="true" /> : null
  const chevronSlot = row ? `Select.chevron` : `Select.trigger`
  const triggerStates = (o: boolean) => [o && `open`, disabled && `disabled`, f.invalid && `invalid`] as const

  if (!multiple && !searchable) {
    const geometry = selectPopupGeometry(props, `--radix-select-trigger-width`)
    return (
      <PlainSelect {...{ node, props, rootProps, part, id, options, value, commit, open, setOpen, cursor, setCursor, disabled, triggerRef, label, rowLabel, spacer, chevronSlot, source, placeholder, hasEmpty, setPlainRoot, triggerStates, geometry }} field={f} />
    )
  }

  return (
    <Combobox
      {...{ node, props, rootProps, part, id, listId, multiple, searchable, placeholder, options, value, commit, open, setOpen, query, setQuery, disabled, triggerRef, label, rowLabel, spacer, chevronSlot, source }}
      localFilter={!optionsBound && !source}
      field={f}
    />
  )
}

interface PlainSelectProps {
  node: NativeProps[`node`]
  props: Record<string, unknown>
  rootProps: Record<string, unknown>
  part: ReturnType<typeof useParts>
  id: string
  options: SelectOption[]
  value: string | string[]
  commit: (next: string | string[]) => void
  open: boolean
  setOpen: (open: boolean) => void
  cursor: string | null
  setCursor: React.Dispatch<React.SetStateAction<string | null>>
  disabled: boolean
  triggerRef: { current: HTMLButtonElement | null }
  label: React.ReactNode
  rowLabel: React.ReactNode
  spacer: React.ReactNode
  chevronSlot: string
  source: string
  placeholder: string
  hasEmpty: boolean
  setPlainRoot: (el: HTMLDivElement | null) => void
  triggerStates: (o: boolean) => readonly (string | false)[]
  geometry: ReturnType<typeof selectPopupGeometry>
  field: ReturnType<typeof useField>
}

function PlainSelect({ node, props, rootProps, part, id, options, value, commit, open, setOpen, cursor, setCursor, disabled, triggerRef, label, rowLabel, spacer, chevronSlot, source, placeholder, hasEmpty, setPlainRoot, triggerStates, geometry, field: f }: PlainSelectProps) {
  const ctx = useSurfaceContext()
  const chosen = options.find((o) => o.value === value)
  const chosenLabel = chosen ? str(chosen.label) : ``
  const valueRef = useRef<HTMLSpanElement | null>(null)
  const viewportRef = useRef<HTMLDivElement | null>(null)
  // #EXP-1629: Radix anchors its trigger: a row popup moves under the value.
  const [offsets, setOffsets] = useState({ sideOffset: geometry.sideOffset, alignOffset: 0 })
  // Radix focuses the chosen item once placed: a fresh opening takes the
  // first enabled one instead, at the top, until the person moves.
  const fresh = useRef(false)
  const [moreBelow, setMoreBelow] = useState(false)
  const measure = () => setMoreBelow(listMoreBelow(viewportRef.current))
  useLayoutEffect(() => {
    if (open) measure()
  }, [open, options.length])
  return (
      <div ref={hasEmpty ? setPlainRoot : undefined} {...(rootProps as Record<string, unknown>)} data-source={source || undefined}>
        {label}
        <SelectPrimitive.Root
          // With an empty option the Select stays CONTROLLED through every
          // pick (`""` is Radix's controlled "no value": the placeholder
          // state, no item checked, the same DOM); a flip to `undefined`
          // and back would make Radix warn. Without one, an unset value
          // stays `undefined` (the hidden native <select> keeps its empty
          // placeholder option, as Solid's).
          value={hasEmpty ? (value as string) : (value as string) || undefined}
          // The option already holding the value fires nothing (Radix's rule;
          // `""` reads as Radix's "no value", so it is checked here).
          onValueChange={(v) => {
            const next = fromRadix(v)
            if (next !== value) commit(next)
          }} disabled={disabled} open={open} onOpenChange={(o) => {
            if (o) {
              fresh.current = true
              const t = triggerRef.current
              const v = valueRef.current
              setOffsets(geometry.anchorValue && t && v ? valueAnchorOffsets(t, v, geometry.align, geometry.sideOffset, ctx.direction === `rtl`) : { sideOffset: geometry.sideOffset, alignOffset: 0 })
            }
            setOpen(o)
            if (!o) setCursor(null)
          }} name={str(props.name, node.id)} dir={ctx.direction}>
          <SelectPrimitive.Trigger ref={triggerRef} id={id} {...(part(`trigger`, ...triggerStates(open)) as Record<string, string>)} aria-invalid={f.invalid || undefined} aria-describedby={f.describedBy}>
            {rowLabel}
            {/* #EXP-1629: the chosen label is the `value` part, Radix's
                Select.Value's own span (it drops a className). */}
            {spacer}
            <span ref={valueRef}>{chosen ? <span {...(part(`value`) as Record<string, string>)} style={{ pointerEvents: `none` }}>{chosenLabel}</span> : <span {...(part(`placeholder`) as Record<string, string>)}>{placeholder}</span>}</span>
            <SelectPrimitive.Icon asChild>
              <BuiltinIcon slot={chevronSlot} part={part(`chevron`)} />
            </SelectPrimitive.Icon>
          </SelectPrimitive.Trigger>
          <SelectPrimitive.Portal container={ctx.portal ?? undefined}>
            <SelectPrimitive.Content
              {...(part(`content`, open && `open`) as Record<string, string>)}
              position="popper"
              align={geometry.align}
              sideOffset={offsets.sideOffset}
              alignOffset={offsets.alignOffset}
              collisionPadding={geometry.collisionPadding}
              data-xui-overlay="Select"
              style={{ minWidth: geometry.minWidth, width: geometry.width }}
              onFocus={(e) => {
                const t = e.target as HTMLElement
                if (!fresh.current || t.getAttribute(`role`) !== `option`) return
                const vp = viewportRef.current
                const first = vp?.querySelector<HTMLElement>(`[role="option"]:not([data-disabled])`)
                if (first && first !== t) first.focus({ preventScroll: true })
                if (vp) vp.scrollTop = 0
              }}
              onKeyDownCapture={() => (fresh.current = false)}
              onPointerMoveCapture={() => (fresh.current = false)}
            >
              <div className="xui-Select-body">
              <SelectPrimitive.Viewport ref={viewportRef} onScroll={measure}>
                {options.length === 0 ? <div {...(part(`empty`) as Record<string, string>)}>{str(props.emptyText) || ctx.t(`noResults`)}</div> : null}
                {options.map((o) => (
                  // #EXP-1629: Radix focuses the cursor's item: that is `hover`.
                  <SelectPrimitive.Item key={o.value} value={toRadix(o.value)} disabled={bool(o.disabled)} {...(part(`item`, o.value === value && `selected`, bool(o.disabled) && `disabled`, o.value === cursor && `hover`) as Record<string, string>)} onFocus={() => setCursor(o.value)} onBlur={() => setCursor((c) => (c === o.value ? null : c))}>
                    {o.icon ? <IconGlyph icons={ctx.host.icons} name={o.icon} /> : null}
                    <SelectPrimitive.ItemText>{str(o.label)}</SelectPrimitive.ItemText>
                    <SelectPrimitive.ItemIndicator {...(part(`check`) as Record<string, string>)} className={`${part(`check`).className as string} xui-Select-check`}>
                      <BuiltinIcon slot="Select.check" size={16} />
                    </SelectPrimitive.ItemIndicator>
                  </SelectPrimitive.Item>
                ))}
              </SelectPrimitive.Viewport>
              {moreBelow ? <div {...(part(`fade`) as Record<string, string>)} aria-hidden="true" /> : null}
              </div>
            </SelectPrimitive.Content>
          </SelectPrimitive.Portal>
        </SelectPrimitive.Root>
        <FieldErrors part={part} field={f} />
      </div>
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
  rowLabel: React.ReactNode
  spacer: React.ReactNode
  chevronSlot: string
  source: string
  localFilter: boolean
  field: ReturnType<typeof useField>
}

function Combobox({ props, rootProps, part, id, listId, multiple, searchable, placeholder, options, value, commit, open, setOpen, query, setQuery, disabled, triggerRef, label, rowLabel, spacer, chevronSlot, source, localFilter, field }: ComboboxProps) {
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
    if (open) nav.setActive(firstEnabledIndex(shown.length, (i) => bool(shown[i]?.disabled)))
    else setQuery(``)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])
  useEffect(() => {
    if (!searchable || !open) return
    nav.setActive(shown.findIndex((o) => !bool(o.disabled)))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query])
  const optionId = (i: number) => `${listId}-${i}`
  const activeId = nav.active >= 0 && nav.active < shown.length ? optionId(nav.active) : undefined
  // #EXP-1629: the list (never the page) scrolls to the cursor.
  useEffect(() => {
    if (open && activeId) revealOption(listRef.current, listRef.current?.querySelector(`[id="${activeId}"]`))
  }, [activeId, open])
  const [moreBelow, setMoreBelow] = useState(false)
  const measure = () => setMoreBelow(listMoreBelow(listRef.current))
  useLayoutEffect(() => {
    if (open) measure()
  }, [open, shown.length])
  const geometry = selectPopupGeometry(props, `--radix-popover-trigger-width`)
  const shownValue = <span>{chosen.length ? <span {...(part(`value`) as Record<string, string>)}>{chosen.map((o) => str(o.label)).join(`, `)}</span> : <span {...(part(`placeholder`) as Record<string, string>)}>{placeholder}</span>}</span>
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
          {rowLabel}
          {spacer}
          {geometry.anchorValue ? <PopoverPrimitive.Anchor asChild>{shownValue}</PopoverPrimitive.Anchor> : shownValue}
          <BuiltinIcon slot={chevronSlot} part={part(`chevron`)} />
        </PopoverPrimitive.Trigger>
        <PopoverPrimitive.Portal container={ctx.portal ?? undefined}>
          <PopoverPrimitive.Content
            {...(part(`content`, open && `open`) as Record<string, string>)}
            dir={ctx.direction}
            align={geometry.align}
            sideOffset={geometry.sideOffset}
            collisionPadding={geometry.collisionPadding}
            data-xui-overlay="Select"
            style={{ minWidth: geometry.minWidth, width: geometry.width }}
            onOpenAutoFocus={(e) => {
              if (!searchable) {
                e.preventDefault()
                listRef.current?.focus()
              }
            }}
          >
            {searchable ? (
              // #EXP-1629: the `search` part is the whole box (glyph + field, one hairline).
              <div {...(part(`search`) as Record<string, string>)} className={`${part(`search`).className as string} xui-Select-searchbox`}>
                <BuiltinIcon slot="Select.search" size={16} />
                <input
                  className="xui-Select-searchfield"
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
            <div className="xui-Select-body">
            <div
              ref={listRef}
              onScroll={measure}
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
                    {...(part(`item`, isSel && `selected`, bool(o.disabled) && `disabled`, isActive && `hover`) as Record<string, string>)}
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
            {moreBelow ? <div {...(part(`fade`) as Record<string, string>)} aria-hidden="true" /> : null}
            </div>
          </PopoverPrimitive.Content>
        </PopoverPrimitive.Portal>
      </PopoverPrimitive.Root>
      <FieldErrors part={part} field={field} />
    </div>
  )
}
