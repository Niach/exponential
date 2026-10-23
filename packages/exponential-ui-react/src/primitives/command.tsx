"use client"

import * as React from "react"
import { CircleXIcon, SearchIcon } from "lucide-react"
import { COMMAND_CLASSES, COMMAND_LABEL_STYLE, COMMAND_SELECT_EVENT, commandItemValue, createCommandStore, observeCommandListHeight, type CommandFilter, type CommandStore } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

// FEED-131: the command menu on the shared engine (no cmdk): the root owns
// the store, every part re-renders on its emit, and the root's layout
// effect runs the post-render pass (`flush`) once the DOM caught up.

interface RootContext {
  store: CommandStore
  listId: string
  inputId: string
  labelId: string
}
const CommandContext = React.createContext<RootContext | null>(null)
const GroupContext = React.createContext<{ id: string; forceMount?: boolean } | null>(null)

function useRoot(): RootContext {
  const ctx = React.useContext(CommandContext)
  if (!ctx) throw new Error(`Command parts must sit inside <Command>`)
  return ctx
}

function useStore(store: CommandStore) {
  React.useSyncExternalStore(store.subscribe, () => store.version, () => store.version)
}

type CommandProps = Omit<React.ComponentProps<`div`>, `onChange` | `defaultValue`> & {
  /** The selected item's value (controlled). */
  value?: string
  defaultValue?: string
  onValueChange?: (value: string) => void
  /** Score an item against the query (0 hides it); default `commandScore`. */
  filter?: CommandFilter
  /** `false` = the host filters: every item shows, in its own order. */
  shouldFilter?: boolean
  /** Arrow keys wrap around. */
  loop?: boolean
  /** The accessible label (a visually hidden <label> for the input). */
  label?: string
}

function Command({ className, value, defaultValue, onValueChange, filter, shouldFilter, loop, label, children, onKeyDown, ...props }: CommandProps) {
  const latest = React.useRef({ value, onValueChange, filter, shouldFilter, loop })
  latest.current = { value, onValueChange, filter, shouldFilter, loop }
  const [, setTick] = React.useState(0)
  const [store] = React.useState(() => {
    const s = createCommandStore({ props: () => latest.current, schedule: () => setTick((t) => t + 1) })
    if (value === undefined && defaultValue !== undefined) s.setValue(defaultValue.trim())
    return s
  })
  useStore(store)
  React.useLayoutEffect(() => {
    store.syncValue()
  }, [store, value])
  React.useLayoutEffect(() => store.flush())
  const listId = React.useId()
  const inputId = React.useId()
  const labelId = React.useId()
  const ctx = React.useMemo(() => ({ store, listId, inputId, labelId }), [store, listId, inputId, labelId])
  return (
    <div
      data-slot="command"
      tabIndex={-1}
      className={cn(COMMAND_CLASSES.root, className)}
      {...props}
      onKeyDown={(e) => {
        onKeyDown?.(e)
        store.onKeyDown(e.nativeEvent)
      }}
    >
      <label data-slot="command-label" htmlFor={inputId} id={labelId} style={COMMAND_LABEL_STYLE}>
        {label}
      </label>
      <CommandContext.Provider value={ctx}>{children}</CommandContext.Provider>
    </div>
  )
}

type CommandInputProps = Omit<React.ComponentProps<`input`>, `value` | `defaultValue` | `onChange` | `type`> & {
  /** The query (controlled). */
  value?: string
  defaultValue?: string
  onValueChange?: (query: string) => void
  /** The clear button's accessible name. */
  clearLabel?: string
}

function CommandInput({ className, value, defaultValue, onValueChange, clearLabel, ref, ...props }: CommandInputProps) {
  const { store, listId, inputId, labelId } = useRoot()
  useStore(store)
  const input = React.useRef<HTMLInputElement | null>(null)
  const controlled = value !== undefined
  React.useState(() => store.setSearch(controlled ? value : (defaultValue ?? ``), true))
  React.useEffect(() => {
    if (controlled) store.setSearch(value)
  }, [controlled, value, store])
  const shown = controlled ? value : store.search
  const change = (next: string) => {
    if (!controlled) store.setSearch(next)
    onValueChange?.(next)
  }
  return (
    <div data-slot="command-input-wrapper" className={COMMAND_CLASSES.inputWrapper}>
      <SearchIcon className={COMMAND_CLASSES.inputGlyph} />
      <input
        ref={(node) => {
          input.current = node
          if (typeof ref === `function`) ref(node)
          else if (ref) ref.current = node
        }}
        data-slot="command-input"
        className={cn(COMMAND_CLASSES.input, className)}
        {...props}
        autoComplete="off"
        autoCorrect="off"
        spellCheck={false}
        aria-autocomplete="list"
        role="combobox"
        aria-expanded={true}
        aria-controls={listId}
        aria-labelledby={labelId}
        aria-activedescendant={store.selectedId}
        id={inputId}
        type="text"
        value={shown}
        onChange={(e) => change(e.target.value)}
      />
      {shown !== `` ? (
        <button
          type="button"
          data-slot="command-input-clear"
          aria-label={clearLabel ?? `Clear search`}
          className={COMMAND_CLASSES.inputClear}
          onClick={() => {
            change(``)
            input.current?.focus()
          }}
        >
          <CircleXIcon />
        </button>
      ) : null}
    </div>
  )
}

function CommandList({ className, children, "aria-label": ariaLabel, ...props }: React.ComponentProps<`div`>) {
  const { store, listId } = useRoot()
  useStore(store)
  const list = React.useRef<HTMLDivElement | null>(null)
  const sizer = React.useRef<HTMLDivElement | null>(null)
  React.useEffect(() => {
    if (list.current && sizer.current) return observeCommandListHeight(list.current, sizer.current)
  }, [])
  return (
    <div ref={list} data-slot="command-list" className={cn(COMMAND_CLASSES.list, className)} {...props} role="listbox" tabIndex={-1} aria-activedescendant={store.selectedId} aria-label={ariaLabel ?? `Suggestions`} id={listId}>
      <div
        ref={(node) => {
          sizer.current = node
          store.setSizer(node)
        }}
        data-slot="command-list-sizer"
      >
        {children}
      </div>
    </div>
  )
}

function CommandEmpty({ className, ...props }: React.ComponentProps<`div`>) {
  const { store } = useRoot()
  useStore(store)
  return store.empty() ? <div data-slot="command-empty" className={cn(COMMAND_CLASSES.empty, className)} {...props} role="presentation" /> : null
}

function CommandGroup({ className, heading, value, forceMount, children, ...props }: Omit<React.ComponentProps<`div`>, `value`> & { heading?: React.ReactNode; value?: string; forceMount?: boolean }) {
  const { store } = useRoot()
  useStore(store)
  const id = React.useId()
  const headingId = React.useId()
  const el = React.useRef<HTMLDivElement | null>(null)
  const [seq] = React.useState(() => store.nextSeq())
  React.useLayoutEffect(() => store.register({ id, kind: `group`, seq }), [store, id, seq])
  React.useLayoutEffect(() => store.update(id, { el: el.current }))
  const visible = forceMount || store.groupVisible(id)
  const groupValue = value ?? (typeof heading === `string` ? heading : undefined)
  const ctx = React.useMemo(() => ({ id, forceMount }), [id, forceMount])
  return (
    <div ref={el} data-slot="command-group" className={cn(COMMAND_CLASSES.group, className)} {...props} role="presentation" data-value={groupValue} hidden={visible ? undefined : true}>
      {heading ? (
        <div data-slot="command-group-heading" className={COMMAND_CLASSES.groupHeading} aria-hidden={true} id={headingId}>
          {heading}
        </div>
      ) : null}
      <div data-slot="command-group-items" role="group" aria-labelledby={heading ? headingId : undefined}>
        <GroupContext.Provider value={ctx}>{children}</GroupContext.Provider>
      </div>
    </div>
  )
}

type CommandItemProps = Omit<React.ComponentProps<`div`>, `value` | `onSelect`> & {
  /** The value it filters and selects by (default: its text, trimmed). */
  value?: string
  keywords?: string[]
  onSelect?: (value: string) => void
  disabled?: boolean
  /** Always rendered, whatever the query. */
  forceMount?: boolean
}

function CommandItem({ className, value, keywords, onSelect, disabled, forceMount, children, ...props }: CommandItemProps) {
  const { store } = useRoot()
  useStore(store)
  const group = React.useContext(GroupContext)
  const id = React.useId()
  const el = React.useRef<HTMLDivElement | null>(null)
  const [seq] = React.useState(() => store.nextSeq())
  const known = React.useRef(``)
  const forced = forceMount ?? group?.forceMount ?? false
  React.useLayoutEffect(() => store.register({ id, kind: `item`, seq, group: group?.id, forceMount: forced }), [store, id, seq, group?.id, forced])
  const own = commandItemValue(value, children, el.current, known.current)
  React.useLayoutEffect(() => {
    const next = commandItemValue(value, children, el.current, known.current)
    known.current = next
    store.update(id, { el: el.current, value: next, keywords: keywords?.map((k) => k.trim()), disabled })
  })
  const select = React.useRef(() => {})
  select.current = () => {
    const v = known.current
    store.setValue(v, { pointer: true })
    onSelect?.(v)
  }
  React.useEffect(() => {
    const node = el.current
    if (!node || disabled) return
    const listener = () => select.current()
    node.addEventListener(COMMAND_SELECT_EVENT, listener)
    return () => node.removeEventListener(COMMAND_SELECT_EVENT, listener)
  })
  const visible = forced || store.score(own, keywords) > 0
  if (!visible) return null
  const selected = own !== `` && store.value === own
  return (
    <div
      ref={el}
      data-slot="command-item"
      className={cn(COMMAND_CLASSES.item, className)}
      {...props}
      id={id}
      role="option"
      aria-disabled={Boolean(disabled)}
      aria-selected={selected}
      data-disabled={Boolean(disabled)}
      data-selected={selected}
      data-value={own || undefined}
      onPointerMove={disabled ? undefined : () => store.setValue(known.current, { pointer: true })}
      onClick={disabled ? undefined : () => select.current()}
    >
      {children}
    </div>
  )
}

function CommandSeparator({ className, alwaysRender, ...props }: React.ComponentProps<`div`> & { alwaysRender?: boolean }) {
  const { store } = useRoot()
  useStore(store)
  const id = React.useId()
  const [seq] = React.useState(() => store.nextSeq())
  const el = React.useRef<HTMLDivElement | null>(null)
  React.useLayoutEffect(() => store.register({ id, kind: `separator`, seq }), [store, id, seq])
  React.useLayoutEffect(() => store.update(id, { el: el.current }))
  if (!alwaysRender && store.search !== ``) return null
  return <div ref={el} data-slot="command-separator" className={cn(COMMAND_CLASSES.separator, className)} {...props} role="separator" />
}

function CommandShortcut({ className, ...props }: React.ComponentProps<`span`>) {
  return <span data-slot="command-shortcut" className={cn(COMMAND_CLASSES.shortcut, className)} {...props} />
}

export { Command, CommandInput, CommandList, CommandEmpty, CommandGroup, CommandItem, CommandSeparator, CommandShortcut }
export type { CommandProps, CommandInputProps, CommandItemProps }
