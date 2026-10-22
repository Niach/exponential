// FEED-128 (React's natives/chips.tsx; round 1 contract §3, a11y.json
// ChipInput + FileUpload).
// ChipInput: token editing — Enter or a comma adds the typed entry (trimmed,
// no duplicates, up to `max`), Backspace in an empty field focuses then
// removes the last chip, each chip has a remove control; `suggestions`
// complete the typing (a listbox driven by the field: ArrowUp/Down +
// Enter). Events: `add {value}`, `remove {value}`, `change {values}`; a
// bound `values` is written.
// FileUpload: a drop zone (a button: Enter/Space opens the picker; drag
// paints the `dragover` state), `accept` + `maxSize` refuse files (the
// message under the field), the BYTES go to the host's `onUpload(files,
// {nodeId, name})`, the event `upload {files: [{name, size, type}]}`; the
// attached `files` (bindable) are listed, each removable (`remove {name}`).

import { For, Show, createMemo, createSignal, createUniqueId, untrack, type Component, type JSX } from "solid-js"
import { acceptsFile, linkHref, partClass } from "@exponential-at/ui/web"
import { useSurfaceContext } from "../context"
import { FieldErrors, createField, joinIds } from "../form"
import type { NativeProps } from "../node-view"
import { createBoundState } from "./bound"
import { arr, bool, BuiltinIcon, formatFileSize, num, objects, parts, phrase, str, TextPart } from "./shared"
import { createListNavigation } from "../behaviors"

export { acceptsFile }

export function ChipInputNative(props: NativeProps): JSX.Element {
  const part = parts(props)
  const ctx = useSurfaceContext()
  const id = createUniqueId()
  const listId = `${id}-suggestions`
  const [values, setValues] = createBoundState<string[]>(
    () => props.node,
    () => props.scope,
    `values`,
    () => arr<unknown>(props.props.values).map(String)
  )
  const [text, setText] = createSignal(``)
  const [focused, setFocused] = createSignal(false)
  let inputEl: HTMLInputElement | undefined
  const f = createField({ node: () => props.node, domId: () => props.domId, props: () => props.props, value: values, focusEl: () => inputEl })
  const disabled = () => bool(props.props.disabled) || Boolean(f.form?.disabled)
  const max = () => (props.props.max === undefined ? Infinity : num(props.props.max, Infinity))
  const suggestions = createMemo(() => objects<{ label: string; value: string; disabled?: boolean }>(props.props.suggestions))
  const shown = createMemo(() => {
    const t = text()
    return t ? suggestions().filter((s) => !values().includes(s.value) && str(s.label).toLowerCase().includes(t.toLowerCase())) : []
  })
  const update = (next: string[]) => {
    setValues(next)
    f.change(next)
    void props.emit(`change`, { values: next }, { values: next })
    return next
  }
  const add = (raw: string) => {
    const value = raw.trim()
    const cur = untrack(values)
    if (!value || cur.includes(value) || cur.length >= untrack(max)) return false
    const next = update([...cur, value])
    void props.emit(`add`, { value }, { values: next })
    setText(``)
    return true
  }
  let listEl: HTMLUListElement | undefined
  const chipAt = (i: number) => listEl?.querySelectorAll<HTMLElement>(`[data-chip]`)[i] ?? null
  const remove = (value: string, focusIndex?: number) => {
    const next = update(untrack(values).filter((v) => v !== value))
    void props.emit(`remove`, { value }, { values: next })
    // Focus the previous chip (keyboard removal) or the field.
    setTimeout(() => {
      const target = focusIndex !== undefined && focusIndex >= 0 ? chipAt(focusIndex) : null
      ;(target ?? inputEl)?.focus()
    }, 0)
  }
  const onChipKey = (e: KeyboardEvent, i: number) => {
    const rtl = ctx.direction === `rtl`
    const prev = rtl ? `ArrowRight` : `ArrowLeft`
    const next = rtl ? `ArrowLeft` : `ArrowRight`
    const cur = untrack(values)
    if (e.key === prev) {
      e.preventDefault()
      chipAt(Math.max(0, i - 1))?.focus()
    } else if (e.key === next) {
      e.preventDefault()
      if (i + 1 < cur.length) chipAt(i + 1)?.focus()
      else inputEl?.focus()
    } else if ((e.key === `Backspace` || e.key === `Delete`) && !untrack(disabled)) {
      e.preventDefault()
      remove(cur[i], e.key === `Backspace` ? i - 1 : i < cur.length - 1 ? i : i - 1)
    }
  }
  const nav = createListNavigation({ count: () => shown().length, isDisabled: (i) => bool(untrack(shown)[i]?.disabled), onPick: (i) => add(untrack(shown)[i].value) })
  const activeId = () => (nav.active() >= 0 && nav.active() < shown().length ? `${listId}-${nav.active()}` : undefined)
  const open = () => focused() && shown().length > 0
  const placeholder = () => str(props.props.placeholder)
  return (
    <div {...props.rootProps}>
      <TextPart part={part(`label`)} text={props.props.label} as="label" htmlFor={id} />
      <div {...part(`field`, focused() && `focus`, disabled() && `disabled`, f.invalid && `invalid`)} onClick={() => inputEl?.focus()} data-xui-chipfield="">
        <ul ref={listEl} class="xui-chip-list" aria-label={str(props.props.label) || undefined}>
          <For each={values()}>
            {(v, i) => (
              <li
                {...part.at(`chip`, i())}
                data-chip=""
                tabindex="-1"
                onKeyDown={(e) => onChipKey(e, i())}
                onFocus={(e) => e.currentTarget.setAttribute(`data-xs`, `focus`)}
                onBlur={(e) => e.currentTarget.removeAttribute(`data-xs`)}
              >
                <span {...part.at(`chipLabel`, i())}>{v}</span>
                <Show when={!disabled()}>
                  <button
                    type="button"
                    tabindex="-1"
                    {...part.at(`remove`, i())}
                    aria-label={phrase(ctx, `removeItem`, { name: v }, () => `${ctx.t(`remove`)} ${v}`)}
                    onClick={(e) => {
                      e.stopPropagation()
                      remove(v)
                    }}
                  >
                    <BuiltinIcon slot="ChipInput.remove" />
                  </button>
                </Show>
              </li>
            )}
          </For>
        </ul>
        <input
          ref={inputEl}
          id={id}
          {...part(`input`)}
          value={text()}
          attr:value={text()}
          disabled={disabled() || values().length >= max()}
          placeholder={values().length === 0 ? placeholder() || undefined : undefined}
          role="combobox"
          aria-autocomplete="list"
          aria-expanded={open()}
          aria-controls={open() ? listId : undefined}
          aria-activedescendant={open() ? activeId() : undefined}
          aria-invalid={f.invalid || undefined}
          aria-describedby={joinIds(f.describedBy)}
          onFocus={() => setFocused(true)}
          onBlur={() => {
            setFocused(false)
            f.touch()
          }}
          onInput={(e) => {
            const v = e.currentTarget.value
            if (v.includes(`,`)) {
              const pieces = v.split(`,`)
              const last = pieces.pop() ?? ``
              for (const p of pieces) add(p)
              setText(last)
              e.currentTarget.value = last
            } else setText(v)
          }}
          onKeyDown={(e) => {
            const t = untrack(text)
            const cur = untrack(values)
            if (untrack(open) && (e.key === `ArrowDown` || e.key === `ArrowUp` || (e.key === `Enter` && untrack(nav.active) >= 0))) {
              nav.onKeyDown(e)
              return
            }
            if (e.key === `Enter`) {
              e.preventDefault()
              if (t.trim()) add(t)
              else f.submit()
            } else if (e.key === `Backspace` && t === `` && cur.length) {
              // First Backspace focuses the last chip; on the chip it removes.
              e.preventDefault()
              chipAt(cur.length - 1)?.focus()
            } else if ((e.key === `ArrowLeft` || e.key === `ArrowRight`) && t === `` && cur.length && e.key === (ctx.direction === `rtl` ? `ArrowRight` : `ArrowLeft`)) {
              e.preventDefault()
              chipAt(cur.length - 1)?.focus()
            } else if (e.key === `Escape`) setText(``)
          }}
        />
      </div>
      <Show when={open()}>
        <div id={listId} role="listbox" class="xui-ChipInput-suggestions">
          <For each={shown()}>
            {(s, i) => (
              <div
                id={`${listId}-${i()}`}
                role="option"
                aria-selected={i() === nav.active()}
                class="xui-ChipInput-suggestion"
                data-active={i() === nav.active() ? `` : undefined}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => add(s.value)}
              >
                {str(s.label)}
              </div>
            )}
          </For>
        </div>
      </Show>
      <FieldErrors part={part} field={f} />
    </div>
  )
}

interface FileMeta {
  name: string
  size?: number
  type?: string
  url?: string
}

export function FileUploadNative(props: NativeProps): JSX.Element {
  const part = parts(props)
  const ctx = useSurfaceContext()
  const id = createUniqueId()
  const [files, setFiles] = createBoundState<FileMeta[]>(
    () => props.node,
    () => props.scope,
    `files`,
    () => objects<FileMeta>(props.props.files)
  )
  let zoneEl: HTMLDivElement | undefined
  let inputEl: HTMLInputElement | undefined
  const f = createField({ node: () => props.node, domId: () => props.domId, props: () => props.props, value: files, focusEl: () => zoneEl })
  const disabled = () => bool(props.props.disabled) || Boolean(f.form?.disabled)
  const multiple = () => bool(props.props.multiple)
  const accept = () => str(props.props.accept)
  const maxSize = () => (props.props.maxSize === undefined ? Infinity : num(props.props.maxSize, Infinity))
  const [drag, setDrag] = createSignal(false)
  const [busy, setBusy] = createSignal(false)
  const [refused, setRefused] = createSignal<string[]>([])
  const name = () => str(props.props.name, props.node.id)
  const size = (n: number | undefined) => (n === undefined ? `` : formatFileSize(n, ctx.locale))
  const take = (list: FileList | File[] | null) => {
    if (!list || untrack(disabled)) return
    const many = untrack(multiple)
    const picked = Array.from(list).slice(0, many ? undefined : 1)
    const ok: File[] = []
    const errors: string[] = []
    for (const file of picked) {
      if (!acceptsFile(untrack(accept), file)) errors.push(phrase(ctx, `unsupportedFileNamed`, { name: file.name }, () => `${file.name}: ${ctx.t(`unsupportedFile`)}`))
      else if (file.size > untrack(maxSize)) errors.push(phrase(ctx, `fileTooLargeNamed`, { name: file.name }, () => `${file.name}: ${ctx.t(`fileTooLarge`)}`))
      else ok.push(file)
    }
    setRefused(errors)
    if (ok.length === 0) return
    const meta = ok.map((file) => ({ name: file.name, size: file.size, type: file.type }))
    const next = many ? [...untrack(files).filter((x) => !meta.some((m) => m.name === x.name)), ...meta] : meta
    setFiles(next)
    f.change(next)
    // The payload = the whole list + the new batch (`added`); the own write
    // = the whole list, as written.
    void props.emit(`upload`, { files: next, added: meta }, { files: next })
    const r = ctx.host.onUpload?.(ok, { nodeId: untrack(() => props.domId), name: untrack(name) })
    if (r && typeof (r as Promise<void>).then === `function`) {
      setBusy(true)
      void (r as Promise<void>).finally(() => setBusy(false))
    }
  }
  const remove = (fileName: string) => {
    const next = untrack(files).filter((x) => x.name !== fileName)
    setFiles(next)
    f.change(next)
    void props.emit(`remove`, { name: fileName, files: next }, { files: next })
  }
  const onDrag = (e: DragEvent, over: boolean) => {
    e.preventDefault()
    if (untrack(disabled)) return
    setDrag(over)
  }
  const hint = () => str(props.props.hint)
  return (
    <div {...props.rootProps}>
      <TextPart part={part(`label`)} text={props.props.label} as="label" htmlFor={id} />
      <div
        ref={zoneEl}
        {...part(`dropzone`, drag() && `dragover`, disabled() && `disabled`, f.invalid && `invalid`)}
        role="button"
        tabindex={disabled() ? -1 : 0}
        aria-disabled={disabled() || undefined}
        aria-busy={busy() || undefined}
        aria-describedby={joinIds(hint() && `${id}-hint`, refused().length > 0 && `${id}-refused`, f.describedBy)}
        aria-label={str(props.props.label) || ctx.t(`dropFiles`)}
        data-dragover={drag() ? `` : undefined}
        onClick={() => !untrack(disabled) && inputEl?.click()}
        onKeyDown={(e) => {
          if (untrack(disabled)) return
          if (e.key === `Enter` || e.key === ` `) {
            e.preventDefault()
            inputEl?.click()
          }
        }}
        onDragEnter={(e) => onDrag(e, true)}
        onDragOver={(e) => onDrag(e, true)}
        onDragLeave={(e) => onDrag(e, false)}
        onDrop={(e) => {
          onDrag(e, false)
          take(e.dataTransfer?.files ?? null)
        }}
      >
        <span {...part(`icon`)}>
          <BuiltinIcon slot="FileUpload.icon" />
        </span>
        <span {...part(`title`)}>{ctx.t(`dropFiles`)}</span>
        <Show when={hint()}>
          <span {...part(`hint`)} id={`${id}-hint`}>
            {hint()}
          </span>
        </Show>
        {/* Round 2 §7: the Browse button inside the zone (an outline small
            Button's look; the zone itself is the control). */}
        <span {...part(`browse`)} class={`${part(`browse`).class as string} ${partClass(`Button`, `root`)}`} data-r-variant="outline" data-r-size="sm" aria-hidden="true">
          {ctx.t(`browse`)}
        </span>
        <input
          ref={inputEl}
          id={id}
          type="file"
          class="xui-sr-only"
          tabindex="-1"
          accept={accept() || undefined}
          multiple={multiple()}
          disabled={disabled()}
          name={name()}
          onChange={(e) => {
            take(e.currentTarget.files)
            e.currentTarget.value = ``
          }}
        />
      </div>
      <Show when={files().length}>
        <ul class="xui-file-list">
          <For each={files()}>
            {(file, i) => {
              const href = () => linkHref(ctx.host, file.url)
              return (
                <li {...part.at(`file`, i())}>
                  <span {...part.at(`fileIcon`, i())}>
                    <BuiltinIcon slot="FileUpload.file" />
                  </span>
                  <span {...part.at(`fileName`, i())}>
                    {href() ? (
                      <a href={href()} target="_blank" rel="noreferrer">
                        {file.name}
                      </a>
                    ) : (
                      file.name
                    )}
                  </span>
                  <Show when={file.size !== undefined}>
                    <span {...part.at(`fileMeta`, i())}>{size(file.size)}</span>
                  </Show>
                  <Show when={!disabled()}>
                    <button type="button" {...part.at(`remove`, i())} aria-label={phrase(ctx, `removeItem`, { name: file.name }, () => `${ctx.t(`remove`)} ${file.name}`)} onClick={() => remove(file.name)}>
                      <BuiltinIcon slot="FileUpload.remove" />
                    </button>
                  </Show>
                </li>
              )
            }}
          </For>
        </ul>
      </Show>
      <Show when={refused().length}>
        <div {...part(`error`)} id={`${id}-refused`} role="alert">
          <For each={refused()}>{(m) => <span class="xui-field-error">{m}</span>}</For>
        </div>
      </Show>
      <FieldErrors part={part} field={f} />
    </div>
  )
}

export const NATIVES_CHIPS: Record<string, Component<NativeProps>> = {
  ChipInput: ChipInputNative,
  FileUpload: FileUploadNative,
}
