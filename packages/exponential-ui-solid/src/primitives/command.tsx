import { Show, children as resolveChildren, createContext, createEffect, createSignal, createUniqueId, on, onCleanup, onMount, splitProps, useContext, type ComponentProps, type JSX } from "solid-js"
import { CircleX, Search } from "lucide-solid"
import { COMMAND_CLASSES, COMMAND_LABEL_STYLE, COMMAND_SELECT_EVENT, commandItemValue, createCommandStore, observeCommandListHeight, type CommandFilter, type CommandStore } from "@exponential_app/eui/web/primitives"

import { PaintIcon } from "../icons"
import { css } from "../style"
import { cn } from "./cn"

// FEED-131: the command menu on the shared engine (React's command.tsx is
// the spec): the root owns the store, every part reads it through the
// version signal, and the post-render pass (`flush`) runs in a microtask
// once the DOM caught up (React: the root's layout effect).

interface RootContext {
  store: CommandStore
  track: () => number
  listId: string
  inputId: string
  labelId: string
}
const CommandContext = createContext<RootContext>()
const GroupContext = createContext<{ id: string; forceMount?: boolean }>()

function useRoot(): RootContext {
  const ctx = useContext(CommandContext)
  if (!ctx) throw new Error(`Command parts must sit inside <Command>`)
  return ctx
}

const LABEL_STYLE = css(COMMAND_LABEL_STYLE)

function callHandler<E extends Event>(handler: unknown, e: E) {
  if (typeof handler === `function`) handler(e)
  else if (Array.isArray(handler)) handler[0](handler[1], e)
}

type CommandProps = Omit<ComponentProps<`div`>, `onChange`> & {
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

function Command(props: CommandProps) {
  const [local, rest] = splitProps(props, [`class`, `value`, `defaultValue`, `onValueChange`, `filter`, `shouldFilter`, `loop`, `label`, `children`, `onKeyDown`])
  const [version, setVersion] = createSignal(0)
  let scheduled = false
  const store: CommandStore = createCommandStore({
    props: () => ({ value: local.value, onValueChange: local.onValueChange, filter: local.filter, shouldFilter: local.shouldFilter, loop: local.loop }),
    schedule: () => {
      if (scheduled) return
      scheduled = true
      queueMicrotask(() => {
        scheduled = false
        store.flush()
      })
    },
  })
  if (local.value === undefined && local.defaultValue !== undefined) store.setValue(local.defaultValue.trim())
  onCleanup(store.subscribe(() => setVersion((v) => v + 1)))
  createEffect(on(() => local.value, () => store.syncValue(), { defer: true }))
  const ctx: RootContext = { store, track: version, listId: createUniqueId(), inputId: createUniqueId(), labelId: createUniqueId() }
  return (
    <div
      data-slot="command"
      tabindex={-1}
      class={cn(COMMAND_CLASSES.root, local.class)}
      {...rest}
      onKeyDown={(e) => {
        callHandler(local.onKeyDown, e)
        store.onKeyDown(e)
      }}
    >
      <label data-slot="command-label" for={ctx.inputId} id={ctx.labelId} style={LABEL_STYLE}>
        {local.label}
      </label>
      <CommandContext.Provider value={ctx}>{local.children}</CommandContext.Provider>
    </div>
  )
}

type CommandInputProps = Omit<ComponentProps<`input`>, `value` | `type` | `ref`> & {
  /** The query (controlled). */
  value?: string
  defaultValue?: string
  onValueChange?: (query: string) => void
  /** The clear button's accessible name. */
  clearLabel?: string
  ref?: (el: HTMLInputElement) => void
}

function CommandInput(props: CommandInputProps) {
  const [local, rest] = splitProps(props, [`class`, `value`, `defaultValue`, `onValueChange`, `clearLabel`, `ref`])
  const { store, track, listId, inputId, labelId } = useRoot()
  let input: HTMLInputElement | undefined
  const controlled = () => local.value !== undefined
  store.setSearch(local.value ?? local.defaultValue ?? ``, true)
  createEffect(
    on(
      () => local.value,
      (v) => {
        if (v !== undefined) store.setSearch(v)
      },
      { defer: true }
    )
  )
  const shown = () => {
    track()
    return controlled() ? local.value! : store.search
  }
  const change = (next: string) => {
    if (!controlled()) store.setSearch(next)
    local.onValueChange?.(next)
  }
  const active = () => {
    track()
    return store.selectedId
  }
  return (
    <div data-slot="command-input-wrapper" class={COMMAND_CLASSES.inputWrapper}>
      <PaintIcon icon={Search} class={COMMAND_CLASSES.inputGlyph} />
      <input
        ref={(el) => {
          input = el
          local.ref?.(el)
        }}
        data-slot="command-input"
        class={cn(COMMAND_CLASSES.input, local.class)}
        {...rest}
        autocomplete="off"
        autocorrect="off"
        spellcheck={false}
        aria-autocomplete="list"
        role="combobox"
        aria-expanded="true"
        aria-controls={listId}
        aria-labelledby={labelId}
        aria-activedescendant={active()}
        id={inputId}
        type="text"
        value={shown()}
        attr:value={shown()}
        onInput={(e) => {
          const next = e.currentTarget.value
          change(next)
          // A controlled query that did not take the keystroke snaps back (React's controlled input).
          if (controlled() && local.value !== next) e.currentTarget.value = local.value!
        }}
      />
      <Show when={shown() !== ``}>
        <button
          type="button"
          data-slot="command-input-clear"
          aria-label={local.clearLabel ?? `Clear search`}
          class={COMMAND_CLASSES.inputClear}
          onClick={() => {
            change(``)
            input?.focus()
          }}
        >
          <PaintIcon icon={CircleX} />
        </button>
      </Show>
    </div>
  )
}

function CommandList(props: ComponentProps<`div`>) {
  const [local, rest] = splitProps(props, [`class`, `children`, `aria-label`])
  const { store, track, listId } = useRoot()
  let list: HTMLDivElement | undefined
  let sizer: HTMLDivElement | undefined
  onMount(() => {
    if (list && sizer) onCleanup(observeCommandListHeight(list, sizer))
  })
  onCleanup(() => store.setSizer(null))
  const active = () => {
    track()
    return store.selectedId
  }
  return (
    <div ref={list} data-slot="command-list" class={cn(COMMAND_CLASSES.list, local.class)} {...rest} role="listbox" tabindex={-1} aria-activedescendant={active()} aria-label={local[`aria-label`] ?? `Suggestions`} id={listId}>
      <div
        ref={(el) => {
          sizer = el
          store.setSizer(el)
        }}
        data-slot="command-list-sizer"
      >
        {local.children}
      </div>
    </div>
  )
}

function CommandEmpty(props: ComponentProps<`div`>) {
  const [local, rest] = splitProps(props, [`class`])
  const { store, track } = useRoot()
  return (
    <Show when={(track(), store.empty())}>
      <div data-slot="command-empty" class={cn(COMMAND_CLASSES.empty, local.class)} {...rest} role="presentation" />
    </Show>
  )
}

/** One registered part's lifecycle: registered once mounted, its element
 *  (null while not rendered) followed. */
function useEntry(store: CommandStore, entry: Parameters<CommandStore[`register`]>[0], el: () => HTMLElement | undefined) {
  onMount(() => {
    onCleanup(store.register(entry))
    store.update(entry.id, { el: el() ?? null })
  })
}

type CommandGroupProps = Omit<ComponentProps<`div`>, `value`> & { heading?: JSX.Element; value?: string; forceMount?: boolean }

function CommandGroup(props: CommandGroupProps) {
  const [local, rest] = splitProps(props, [`class`, `heading`, `value`, `forceMount`, `children`])
  const { store, track } = useRoot()
  const id = createUniqueId()
  const headingId = createUniqueId()
  const seq = store.nextSeq()
  let el: HTMLDivElement | undefined
  useEntry(store, { id, kind: `group`, seq }, () => el)
  const visible = () => {
    track()
    return Boolean(local.forceMount) || store.groupVisible(id)
  }
  const heading = resolveChildren(() => local.heading)
  const groupValue = () => local.value ?? (typeof local.heading === `string` ? local.heading : undefined)
  return (
    <div ref={el} data-slot="command-group" class={cn(COMMAND_CLASSES.group, local.class)} {...rest} role="presentation" data-value={groupValue()} hidden={!visible()}>
      <Show when={heading()}>
        <div data-slot="command-group-heading" class={COMMAND_CLASSES.groupHeading} aria-hidden="true" id={headingId}>
          {heading()}
        </div>
      </Show>
      <div data-slot="command-group-items" role="group" aria-labelledby={heading() ? headingId : undefined}>
        <GroupContext.Provider
          value={{
            id,
            get forceMount() {
              return local.forceMount
            },
          }}
        >
          {local.children}
        </GroupContext.Provider>
      </div>
    </div>
  )
}

type CommandItemProps = Omit<ComponentProps<`div`>, `onSelect`> & {
  /** The value it filters and selects by (default: its text, trimmed). */
  value?: string
  keywords?: string[]
  onSelect?: (value: string) => void
  disabled?: boolean
  /** Always rendered, whatever the query. */
  forceMount?: boolean
}

function CommandItem(props: CommandItemProps) {
  const [local, rest] = splitProps(props, [`class`, `value`, `keywords`, `onSelect`, `disabled`, `forceMount`, `children`])
  const { store, track } = useRoot()
  const group = useContext(GroupContext)
  const id = createUniqueId()
  const seq = store.nextSeq()
  let el: HTMLDivElement | undefined
  let known = ``
  const kids = resolveChildren(() => local.children)
  const forced = () => local.forceMount ?? group?.forceMount ?? false
  useEntry(store, { id, kind: `item`, seq, group: group?.id, forceMount: forced() }, () => el)
  const own = () => {
    track()
    const k = kids()
    return commandItemValue(local.value, typeof k === `string` ? k : undefined, el, known)
  }
  createEffect(() => {
    const next = own()
    known = next
    store.update(id, { el: el ?? null, value: next, keywords: local.keywords?.map((k) => k.trim()), disabled: local.disabled })
  })
  const select = () => {
    const v = known
    store.setValue(v, { pointer: true })
    local.onSelect?.(v)
  }
  const visible = () => forced() || store.score(own(), local.keywords) > 0
  const selected = () => own() !== `` && store.value === own()
  return (
    <Show when={visible()}>
      {(() => {
        const node = (
          <div
            data-slot="command-item"
            class={cn(COMMAND_CLASSES.item, local.class)}
            {...rest}
            id={id}
            role="option"
            aria-disabled={Boolean(local.disabled)}
            aria-selected={selected()}
            data-disabled={Boolean(local.disabled)}
            data-selected={selected()}
            data-value={own() || undefined}
            onPointerMove={local.disabled ? undefined : () => store.setValue(known, { pointer: true })}
            onClick={local.disabled ? undefined : select}
          >
            {kids()}
          </div>
        ) as HTMLDivElement
        el = node
        const listener = () => {
          if (!local.disabled) select()
        }
        node.addEventListener(COMMAND_SELECT_EVENT, listener)
        onCleanup(() => {
          node.removeEventListener(COMMAND_SELECT_EVENT, listener)
          if (el === node) el = undefined
          store.update(id, { el: null })
        })
        store.update(id, { el: node })
        return node
      })()}
    </Show>
  )
}

function CommandSeparator(props: ComponentProps<`div`> & { alwaysRender?: boolean }) {
  const [local, rest] = splitProps(props, [`class`, `alwaysRender`])
  const { store, track } = useRoot()
  const id = createUniqueId()
  const seq = store.nextSeq()
  let el: HTMLDivElement | undefined
  useEntry(store, { id, kind: `separator`, seq }, () => el)
  return (
    <Show when={(track(), local.alwaysRender || store.search === ``)}>
      {(() => {
        const node = (<div data-slot="command-separator" class={cn(COMMAND_CLASSES.separator, local.class)} {...rest} role="separator" />) as HTMLDivElement
        el = node
        store.update(id, { el: node })
        onCleanup(() => {
          if (el === node) el = undefined
          store.update(id, { el: null })
        })
        return node
      })()}
    </Show>
  )
}

function CommandShortcut(props: ComponentProps<`span`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <span data-slot="command-shortcut" class={cn(COMMAND_CLASSES.shortcut, local.class)} {...rest} />
}

export { Command, CommandInput, CommandList, CommandEmpty, CommandGroup, CommandItem, CommandSeparator, CommandShortcut }
export type { CommandProps, CommandInputProps, CommandItemProps, CommandGroupProps }
