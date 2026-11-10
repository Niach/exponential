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
// the value and the chevron sit in the `button` part (the IDE picker's ghost
// value button: its hover is theirs too); every popup opens at the top with
// the cursor on the first enabled option (a `multiple` one's on its first
// chosen; the search field focused when searchable) and fades its list's bottom
// while options sit below; an item's `hover` = the cursor, `selected` =
// chosen. A `description` is a muted hint: under a field trigger, in a
// row UNDER the label (the `labels` column; the value and chevron then sit
// in the trailing `control` column). An option's `trailing` reads on after
// its label in the popup row only (the trigger shows the label); `open`
// fires as the popup opens. #EXP-1765: a `pill` trigger is the field
// trigger content-sized, no label part (the label = its `aria-label`), the
// `Select.dropdown` glyph; its popup a hairline outside it, opened by the
// pointer with no cursor (the keyboard keeps it). `loading` fills the list
// with the Skeleton's rows (`<id>.skeleton`); a `mono` option sets its
// label in the mono type and its `trailing` as its own muted sans run
// (aria-hidden: the label alone names the option);
// `searchPlaceholder` hints the filter field; the `searchIcon` part sizes
// its glyph.

import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react"
import { Popover as PopoverPrimitive, Select as SelectPrimitive } from "radix-ui"
import { nativeRecipeProps, resolveRecipe } from "@exponential_app/eui"
import { useSurfaceContext } from "../context"
import { boundPath, insetPopupNudge, listMoreBelow, openingCursorIndex, revealOption, selectClearSize, selectLoadingNode, selectOpensWithoutCursor, selectPopupGeometry, valueAnchorOffsets } from "@exponential_app/eui/web"
import { DEBOUNCE_MS } from "../inputs"
import { FieldErrors, joinIds, useField } from "../form"
import { IconGlyph } from "../icons"
import { NodeView, type NativeProps } from "../node-view"
import { useBoundState } from "./bound"
import { useListNavigation } from "./listbox"
import { bool, BuiltinIcon, str, useParts, TextPart, objects } from "./shared"

export interface SelectOption {
  label: string
  value: string
  icon?: string
  disabled?: boolean
  /** #EXP-1629: reads on after the label in the popup row (`main · default`). */
  trailing?: string
  /** #EXP-1765: the label in the mono type, the `trailing` its own muted run. */
  mono?: boolean
  /** #EXP-1629: a muted second line under the label (the IDE picker's `Offline`). */
  description?: string
}

/** #EXP-1629: an option's `trailing` after its label, one space apart (the
 *  item's own style); #EXP-1765: a `mono` option's is its own run instead. */
const trailingText = (o: SelectOption) => (str(o.trailing) && !bool(o.mono) ? ` ${str(o.trailing)}` : ``)

/** #EXP-1765: a `mono` option's `trailing` run (muted, the sans type, the item's gap apart). */
const TrailingRun = ({ o }: { o: SelectOption }) => (bool(o.mono) && str(o.trailing) ? <span className="xui-Select-trailing" aria-hidden="true">{str(o.trailing)}</span> : null)

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

  // #EXP-1629: `open` as the popup opens (the IDE picker's lazy fetch).
  useEffect(() => {
    if (open) void emit(`open`)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

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
  // #EXP-1765: a `pill` shows no label (it names the trigger) and no hint.
  const pill = str(props.triggerVariant) === `pill`
  const label = row || pill ? null : <TextPart part={part(`label`)} text={props.label} as="label" htmlFor={id} />
  const description = str(props.description)
  const described = row && description !== ``
  const rowLabel = !row ? null : described ? (
    <div {...(part(`labels`) as Record<string, string>)}>
      <TextPart part={part(`label`)} text={props.label} />
      <TextPart part={part(`description`)} text={description} id={`${id}-d`} />
    </div>
  ) : (
    <TextPart part={part(`label`)} text={props.label} />
  )
  const spacer = row && !described ? <span className="xui-Select-spacer" aria-hidden="true" /> : null
  // #EXP-1629: the row's value button lends its hover to what it holds.
  const [buttonHover, setButtonHover] = useState(false)
  const inButton = row && buttonHover && `hover`
  const button = (children: React.ReactNode) =>
    row ? (
      <span {...(part(`button`) as Record<string, string>)} onPointerEnter={() => setButtonHover(true)} onPointerLeave={() => setButtonHover(false)}>
        {children}
      </span>
    ) : (
      children
    )
  const hint: Hint = {
    end: (children) => (described ? <div {...(part(`control`) as Record<string, string>)}>{button(children)}</div> : button(children)),
    field: row || pill ? null : <TextPart part={part(`description`)} text={description} id={`${id}-d`} />,
    describedBy: joinIds(!pill && description && `${id}-d`, f.describedBy),
    ariaLabel: pill ? str(props.label) || undefined : undefined,
  }
  const chevronSlot = row ? `Select.chevron` : pill ? `Select.dropdown` : `Select.trigger`
  // #EXP-1765: how the popup last opened (a pointer-opened pill has no cursor).
  const byPointer = useRef(false)
  const searchPlaceholder = str(props.searchPlaceholder) || ctx.t(`search`)
  const loading = bool(props.loading)
  const skeleton = useMemo(() => selectLoadingNode(node.id), [node.id])
  const triggerStates = (o: boolean) => [o && `open`, disabled && `disabled`, f.invalid && `invalid`] as const

  if (!multiple && !searchable) {
    const geometry = selectPopupGeometry(props, `--radix-select-trigger-width`)
    return (
      <PlainSelect {...{ node, props, rootProps, part, id, options, value, commit, open, setOpen, cursor, setCursor, disabled, triggerRef, label, rowLabel, spacer, chevronSlot, source, placeholder, hasEmpty, setPlainRoot, triggerStates, geometry, hint, inButton, byPointer, loading, skeleton }} field={f} />
    )
  }

  return (
    <Combobox
      {...{ node, props, rootProps, part, id, listId, multiple, searchable, placeholder, options, value, commit, open, setOpen, query, setQuery, disabled, triggerRef, label, rowLabel, spacer, chevronSlot, source, hint, inButton, byPointer, loading, skeleton, searchPlaceholder }}
      localFilter={!optionsBound && !source}
      field={f}
    />
  )
}

/** A `description`: the row's `control` wrapper, the field's hint line, the trigger's describedby. */
interface Hint {
  end: (children: React.ReactNode) => React.ReactNode
  field: React.ReactNode
  describedBy: string | undefined
  /** #EXP-1765: a pill's label names its trigger. */
  ariaLabel: string | undefined
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
  hint: Hint
  chevronSlot: string
  source: string
  placeholder: string
  hasEmpty: boolean
  setPlainRoot: (el: HTMLDivElement | null) => void
  triggerStates: (o: boolean) => readonly (string | false)[]
  geometry: ReturnType<typeof selectPopupGeometry>
  inButton: string | false
  /** #EXP-1765: did the pointer open the popup (a pill then has no cursor)? */
  byPointer: { current: boolean }
  loading: boolean
  skeleton: NativeProps[`node`]
  field: ReturnType<typeof useField>
}

/** #EXP-1629: a pill popup lands where `placeInsetOverlay` says (the inset,
 *  the clamp, then whole px from the pill): Radix's popper takes no
 *  middleware and rounds its wrapper to device px, so the content moves
 *  (`translate`) from the wrapper onto that box each time Radix places it
 *  (and as the pill, the popup or the window moves or resizes: a move
 *  under a device px leaves Radix's wrapper where it was). */
function useInsetNudge(content: HTMLElement | null, trigger: { current: HTMLElement | null }, geometry: ReturnType<typeof selectPopupGeometry>): void {
  const { align, sideOffset, collisionPadding, inset } = geometry
  useLayoutEffect(() => {
    const wrapper = content?.closest<HTMLElement>(`[data-radix-popper-content-wrapper]`)
    if (!content || !wrapper || inset === undefined) return
    const apply = () => {
      const t = trigger.current
      // Radix parks an unplaced wrapper at translate(0, -200%).
      const n = t && !wrapper.style.transform.includes(`-200%`) ? insetPopupNudge(t, wrapper, { align, sideOffset, collisionPadding, inset }) : null
      content.style.translate = n && (n.x || n.y) ? `${n.x}px ${n.y}px` : ``
    }
    apply()
    const watch = new MutationObserver(apply)
    watch.observe(wrapper, { attributes: true, attributeFilter: [`style`] })
    const sized = typeof ResizeObserver === `undefined` ? null : new ResizeObserver(apply)
    sized?.observe(wrapper)
    if (trigger.current) sized?.observe(trigger.current)
    const win = wrapper.ownerDocument.defaultView
    win?.addEventListener(`scroll`, apply, { capture: true, passive: true })
    win?.addEventListener(`resize`, apply)
    return () => {
      watch.disconnect()
      sized?.disconnect()
      win?.removeEventListener(`scroll`, apply, { capture: true })
      win?.removeEventListener(`resize`, apply)
    }
  }, [content, trigger, align, sideOffset, collisionPadding, inset])
}

function PlainSelect({ node, props, rootProps, part, id, options, value, commit, open, setOpen, cursor, setCursor, disabled, triggerRef, label, rowLabel, spacer, chevronSlot, source, placeholder, hasEmpty, setPlainRoot, triggerStates, geometry, hint, inButton, byPointer, loading, skeleton, field: f }: PlainSelectProps) {
  const ctx = useSurfaceContext()
  const chosen = options.find((o) => o.value === value)
  const chosenLabel = chosen ? str(chosen.label) : ``
  const valueRef = useRef<HTMLSpanElement | null>(null)
  const viewportRef = useRef<HTMLDivElement | null>(null)
  // #EXP-1629: Radix anchors its trigger: a row popup moves under the value.
  const [offsets, setOffsets] = useState({ sideOffset: geometry.sideOffset, alignOffset: geometry.alignOffset })
  // Radix focuses the chosen item once placed: a fresh opening takes the
  // first enabled one instead, at the top, until the person moves
  // (#EXP-1765: a pointer-opened pill's none: the content keeps focus).
  const fresh = useRef(false)
  const bare = useRef(false)
  const [moreBelow, setMoreBelow] = useState(false)
  const measure = () => setMoreBelow(listMoreBelow(viewportRef.current))
  useLayoutEffect(() => {
    if (open) measure()
  }, [open, options.length])
  const [content, setContent] = useState<HTMLDivElement | null>(null)
  useInsetNudge(content, triggerRef, geometry)
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
              bare.current = selectOpensWithoutCursor(props, byPointer.current)
              const t = triggerRef.current
              const v = valueRef.current
              setOffsets(geometry.anchorValue && t && v ? valueAnchorOffsets(t, v, geometry.align, geometry.sideOffset, ctx.direction === `rtl`) : { sideOffset: geometry.sideOffset, alignOffset: geometry.alignOffset })
            }
            setOpen(o)
            if (!o) setCursor(null)
          }} name={str(props.name, node.id)} dir={ctx.direction}>
          <SelectPrimitive.Trigger ref={triggerRef} id={id} {...(part(`trigger`, ...triggerStates(open)) as Record<string, string>)} aria-invalid={f.invalid || undefined} aria-describedby={hint.describedBy} aria-label={hint.ariaLabel} onPointerDown={() => (byPointer.current = true)} onKeyDown={() => (byPointer.current = false)}>
            {rowLabel}
            {/* #EXP-1629: the chosen label is the `value` part, Radix's
                Select.Value's own span (it drops a className). */}
            {spacer}
            {hint.end(
              <>
                <span ref={valueRef}>{chosen ? <span {...(part.with(`value`, { disabled }, inButton) as Record<string, string>)} style={{ pointerEvents: `none` }}>{chosenLabel}</span> : <span {...(part.with(`placeholder`, { disabled }, inButton) as Record<string, string>)}>{placeholder}</span>}</span>
                <SelectPrimitive.Icon asChild>
                  <BuiltinIcon slot={chevronSlot} part={part(`chevron`, inButton)} />
                </SelectPrimitive.Icon>
              </>
            )}
          </SelectPrimitive.Trigger>
          <SelectPrimitive.Portal container={ctx.portal ?? undefined}>
            <SelectPrimitive.Content
              ref={setContent}
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
                if (bare.current) {
                  ;(e.currentTarget as HTMLElement).focus({ preventScroll: true })
                  if (vp) vp.scrollTop = 0
                  return
                }
                const first = vp?.querySelector<HTMLElement>(`[role="option"]:not([data-disabled])`)
                if (first && first !== t) first.focus({ preventScroll: true })
                if (vp) vp.scrollTop = 0
              }}
              onKeyDownCapture={() => (fresh.current = false)}
              onPointerMoveCapture={() => (fresh.current = false)}
            >
              <div className="xui-Select-body">
              <SelectPrimitive.Viewport ref={viewportRef} onScroll={measure}>
                {loading ? <NodeView node={skeleton} /> : null}
                {!loading && options.length === 0 ? <div {...(part(`empty`) as Record<string, string>)}>{str(props.emptyText) || ctx.t(`noResults`)}</div> : null}
                {(loading ? [] : options).map((o) => (
                  // #EXP-1629: Radix focuses the cursor's item: that is `hover`.
                  <SelectPrimitive.Item key={o.value} value={toRadix(o.value)} disabled={bool(o.disabled)} {...(part(`item`, o.value === value && `selected`, bool(o.disabled) && `disabled`, o.value === cursor && `hover`) as Record<string, string>)} data-mono={bool(o.mono) ? `` : undefined} onFocus={() => setCursor(o.value)} onBlur={() => setCursor((c) => (c === o.value ? null : c))}>
                    {o.icon ? <span {...(part(`itemIcon`) as Record<string, string>)}><IconGlyph icons={ctx.host.icons} name={o.icon} /></span> : null}
                    {str(o.description) ? (
                      <span {...(part(`itemText`) as Record<string, string>)}>
                        <span {...(part(`itemLabel`) as Record<string, string>)}><SelectPrimitive.ItemText>{str(o.label)}</SelectPrimitive.ItemText>{trailingText(o)}</span>
                        <span {...(part(`itemDescription`) as Record<string, string>)}>{str(o.description)}</span>
                      </span>
                    ) : trailingText(o) ? (
                      <span>
                        <SelectPrimitive.ItemText>{str(o.label)}</SelectPrimitive.ItemText>
                        {trailingText(o)}
                      </span>
                    ) : (
                      <SelectPrimitive.ItemText>{str(o.label)}</SelectPrimitive.ItemText>
                    )}
                    <TrailingRun o={o} />
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
        {hint.field}
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
  hint: Hint
  chevronSlot: string
  source: string
  localFilter: boolean
  searchPlaceholder: string
  inButton: string | false
  /** #EXP-1765: did the pointer open the popup (a pill then has no cursor)? */
  byPointer: { current: boolean }
  loading: boolean
  skeleton: NativeProps[`node`]
  field: ReturnType<typeof useField>
}

function Combobox({ props, rootProps, part, id, listId, multiple, searchable, placeholder, options, value, commit, open, setOpen, query, setQuery, disabled, triggerRef, label, rowLabel, spacer, chevronSlot, source, hint, inButton, byPointer, loading, skeleton, searchPlaceholder, localFilter, field }: ComboboxProps) {
  const ctx = useSurfaceContext()
  const selected = multiple ? (value as string[]) : value ? [value as string] : []
  const chosen = options.filter((o) => selected.includes(o.value))
  // #EXP-1765: while `loading` the list holds the skeleton, no option.
  const shown = useMemo(() => (loading ? [] : query && localFilter ? options.filter((o) => str(o.label).toLowerCase().includes(query.toLowerCase())) : options), [options, query, localFilter, loading])
  const listRef = useRef<HTMLDivElement | null>(null)
  const searchRef = useRef<HTMLInputElement | null>(null)
  // #EXP-1765: a pill's search shows a CLEAR button while it holds text.
  const pill = str(props.triggerVariant) === `pill`
  const clear = pill ? selectClearSize(resolveRecipe(ctx.theme, { component: `Select`, part: `searchIcon`, props: nativeRecipeProps(`Select`, props, ctx.extensionDefs) }, ctx.mode).width) : null
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
    if (open) nav.setActive(selectOpensWithoutCursor(props, byPointer.current) ? -1 : openingCursorIndex(shown.length, (i) => bool(shown[i]?.disabled), multiple ? (i) => selected.includes(shown[i]!.value) : undefined))
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
  const [content, setContent] = useState<HTMLDivElement | null>(null)
  useInsetNudge(content, triggerRef, geometry)
  const shownValue = <span>{chosen.length ? <span {...(part.with(`value`, { disabled }, inButton) as Record<string, string>)}>{chosen.map((o) => str(o.label)).join(`, `)}</span> : <span {...(part.with(`placeholder`, { disabled }, inButton) as Record<string, string>)}>{placeholder}</span>}</span>
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
          aria-describedby={hint.describedBy}
          aria-label={hint.ariaLabel}
          onPointerDown={() => (byPointer.current = true)}
          onKeyDown={(e) => {
            byPointer.current = false
            if (!open && (e.key === `ArrowDown` || e.key === `ArrowUp`)) {
              e.preventDefault()
              setOpen(true)
            }
          }}
        >
          {rowLabel}
          {spacer}
          {hint.end(
            <>
              {geometry.anchorValue ? <PopoverPrimitive.Anchor asChild>{shownValue}</PopoverPrimitive.Anchor> : shownValue}
              <BuiltinIcon slot={chevronSlot} part={part(`chevron`, inButton)} />
            </>
          )}
        </PopoverPrimitive.Trigger>
        <PopoverPrimitive.Portal container={ctx.portal ?? undefined}>
          <PopoverPrimitive.Content
            ref={setContent}
            {...(part(`content`, open && `open`) as Record<string, string>)}
            dir={ctx.direction}
            align={geometry.align}
            sideOffset={geometry.sideOffset}
            alignOffset={geometry.alignOffset}
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
                <BuiltinIcon slot="Select.search" part={part(`searchIcon`)} />
                <input
                  ref={searchRef}
                  className="xui-Select-searchfield"
                  role="combobox"
                  aria-expanded
                  aria-controls={listId}
                  aria-activedescendant={activeId}
                  aria-autocomplete="list"
                  placeholder={searchPlaceholder}
                  aria-label={ctx.t(`search`)}
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  onKeyDown={nav.onKeyDown}
                />
                {clear && query ? (
                  <button
                    type="button"
                    className="xui-Select-clear"
                    aria-label={ctx.t(`clearSearch`)}
                    style={{ width: clear.box, height: clear.box }}
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => {
                      setQuery(``)
                      searchRef.current?.focus()
                    }}
                  >
                    <BuiltinIcon slot="Select.clear" size={clear.glyph} />
                  </button>
                ) : null}
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
              {loading ? <NodeView node={skeleton} /> : shown.length === 0 ? <div {...(part(`empty`) as Record<string, string>)}>{str(props.emptyText) || ctx.t(`noResults`)}</div> : null}
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
                    data-mono={bool(o.mono) ? `` : undefined}
                    onPointerMove={() => nav.setActive(i)}
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => pick(i)}
                  >
                    {o.icon ? <span {...(part(`itemIcon`) as Record<string, string>)}><IconGlyph icons={ctx.host.icons} name={o.icon} /></span> : null}
                    {str(o.description) ? (
                      <span {...(part(`itemText`) as Record<string, string>)}>
                        <span {...(part(`itemLabel`) as Record<string, string>)}>{str(o.label)}{trailingText(o)}</span>
                        <span {...(part(`itemDescription`) as Record<string, string>)}>{str(o.description)}</span>
                      </span>
                    ) : (
                      <span>{str(o.label)}{trailingText(o)}</span>
                    )}
                    <TrailingRun o={o} />
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
      {hint.field}
      <FieldErrors part={part} field={field} />
    </div>
  )
}
