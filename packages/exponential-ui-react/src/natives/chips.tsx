// Round 1 (contract §3, a11y.json ChipInput + FileUpload).
// ChipInput: token editing — Enter or a comma adds the typed entry (trimmed,
// no duplicates, up to `max`), Backspace in an empty field removes the last
// chip, each chip has a remove control; `suggestions` complete the typing
// (a listbox driven by the field: ArrowUp/Down + Enter). Events: `add
// {value}`, `remove {value}`, `change {values}`; a bound `values` is written.
// FileUpload: a drop zone (a button: Enter/Space opens the picker; drag
// paints the `dragover` state), `accept` + `maxSize` refuse files (the
// message under the field), the BYTES go to the host's `onUpload(files,
// {nodeId, name})`, the event `upload {files: [{name, size, type}]}`; the
// attached `files` (bindable) are listed, each removable (`remove {name}`).

import { useId, useMemo, useRef, useState, type DragEvent } from "react"
import { useSurfaceContext } from "../context"
import { FieldErrors, joinIds, useField } from "../form"
import type { NativeProps } from "../node-view"
import { useBoundState } from "./bound"
import { useListNavigation } from "./listbox"
import { partClass } from "../theme-css"
import { arr, bool, BuiltinIcon, formatFileSize, num, phrase, str, useParts, TextPart } from "./shared"

export function ChipInputNative({ node, props, rootProps, emit, scope, domId }: NativeProps) {
  const part = useParts(node, props)
  const ctx = useSurfaceContext()
  const id = useId()
  const listId = `${id}-suggestions`
  const external = arr<unknown>(props.values).map(String)
  const [values, setValues] = useBoundState<string[]>(node, scope, `values`, external)
  const [text, setText] = useState(``)
  const [focused, setFocused] = useState(false)
  const inputRef = useRef<HTMLInputElement | null>(null)
  const f = useField({ node, domId, props, value: values, focusRef: inputRef })
  const disabled = bool(props.disabled) || Boolean(f.form?.disabled)
  const max = props.max === undefined ? Infinity : num(props.max, Infinity)
  const suggestions = arr<{ label: string; value: string; disabled?: boolean }>(props.suggestions)
  const shown = useMemo(() => (text ? suggestions.filter((s) => !values.includes(s.value) && str(s.label).toLowerCase().includes(text.toLowerCase())) : []), [suggestions, text, values])
  const update = (next: string[]) => {
    setValues(next)
    f.change(next)
    void emit(`change`, { values: next })
  }
  const add = (raw: string) => {
    const value = raw.trim()
    if (!value || values.includes(value) || values.length >= max) return false
    update([...values, value])
    void emit(`add`, { value })
    setText(``)
    return true
  }
  const listRef = useRef<HTMLUListElement | null>(null)
  const chipAt = (i: number) => listRef.current?.querySelectorAll<HTMLElement>(`[data-chip]`)[i] ?? null
  const remove = (value: string, focusIndex?: number) => {
    update(values.filter((v) => v !== value))
    void emit(`remove`, { value })
    // Focus the previous chip (keyboard removal) or the field.
    setTimeout(() => {
      const target = focusIndex !== undefined && focusIndex >= 0 ? chipAt(focusIndex) : null
      ;(target ?? inputRef.current)?.focus()
    }, 0)
  }
  const onChipKey = (e: React.KeyboardEvent, i: number) => {
    const rtl = ctx.direction === `rtl`
    const prev = rtl ? `ArrowRight` : `ArrowLeft`
    const next = rtl ? `ArrowLeft` : `ArrowRight`
    if (e.key === prev) {
      e.preventDefault()
      chipAt(Math.max(0, i - 1))?.focus()
    } else if (e.key === next) {
      e.preventDefault()
      if (i + 1 < values.length) chipAt(i + 1)?.focus()
      else inputRef.current?.focus()
    } else if ((e.key === `Backspace` || e.key === `Delete`) && !disabled) {
      e.preventDefault()
      remove(values[i], e.key === `Backspace` ? i - 1 : i < values.length - 1 ? i : i - 1)
    }
  }
  const nav = useListNavigation({ count: shown.length, isDisabled: (i) => bool(shown[i]?.disabled), onPick: (i) => add(shown[i].value) })
  const activeId = nav.active >= 0 && nav.active < shown.length ? `${listId}-${nav.active}` : undefined
  const open = focused && shown.length > 0
  const placeholder = str(props.placeholder)
  return (
    <div {...(rootProps as Record<string, unknown>)}>
      <TextPart part={part(`label`)} text={props.label} as="label" htmlFor={id} />
      <div {...(part(`field`, focused && `focus`, disabled && `disabled`, f.invalid && `invalid`) as Record<string, string>)} onClick={() => inputRef.current?.focus()} data-xui-chipfield="">
        <ul ref={listRef} className="xui-chip-list" aria-label={str(props.label) || undefined}>
          {values.map((v, i) => (
            <li key={v} {...(part.at(`chip`, i) as Record<string, string>)} data-chip="" tabIndex={-1} onKeyDown={(e) => onChipKey(e, i)} onFocus={(e) => e.currentTarget.setAttribute(`data-xs`, `focus`)} onBlur={(e) => e.currentTarget.removeAttribute(`data-xs`)}>
              <span {...(part.at(`chipLabel`, i) as Record<string, string>)}>{v}</span>
              {disabled ? null : (
                <button type="button" tabIndex={-1} {...(part.at(`remove`, i) as Record<string, string>)} aria-label={phrase(ctx, `removeItem`, { name: v }, () => `${ctx.t(`remove`)} ${v}`)} onClick={(e) => {
                  e.stopPropagation()
                  remove(v)
                }}>
                  <BuiltinIcon slot="ChipInput.remove" />
                </button>
              )}
            </li>
          ))}
        </ul>
        <input
          ref={inputRef}
          id={id}
          {...(part(`input`) as Record<string, string>)}
          value={text}
          disabled={disabled || values.length >= max}
          placeholder={values.length === 0 ? placeholder || undefined : undefined}
          role="combobox"
          aria-autocomplete="list"
          aria-expanded={open}
          aria-controls={open ? listId : undefined}
          aria-activedescendant={open ? activeId : undefined}
          aria-invalid={f.invalid || undefined}
          aria-describedby={joinIds(f.describedBy)}
          onFocus={() => setFocused(true)}
          onBlur={() => {
            setFocused(false)
            f.touch()
          }}
          onChange={(e) => {
            const v = e.target.value
            if (v.includes(`,`)) {
              const parts = v.split(`,`)
              const last = parts.pop() ?? ``
              for (const p of parts) add(p)
              setText(last)
            } else setText(v)
          }}
          onKeyDown={(e) => {
            if (open && (e.key === `ArrowDown` || e.key === `ArrowUp` || (e.key === `Enter` && nav.active >= 0))) {
              nav.onKeyDown(e)
              return
            }
            if (e.key === `Enter`) {
              e.preventDefault()
              if (text.trim()) add(text)
              else f.submit()
            } else if (e.key === `Backspace` && text === `` && values.length) {
              // First Backspace focuses the last chip; on the chip it removes.
              e.preventDefault()
              chipAt(values.length - 1)?.focus()
            } else if ((e.key === `ArrowLeft` || e.key === `ArrowRight`) && text === `` && values.length && e.key === (ctx.direction === `rtl` ? `ArrowRight` : `ArrowLeft`)) {
              e.preventDefault()
              chipAt(values.length - 1)?.focus()
            } else if (e.key === `Escape`) setText(``)
          }}
        />
      </div>
      {open ? (
        <div id={listId} role="listbox" className="xui-ChipInput-suggestions">
          {shown.map((s, i) => (
            <div key={s.value} id={`${listId}-${i}`} role="option" aria-selected={i === nav.active} className="xui-ChipInput-suggestion" data-active={i === nav.active ? `` : undefined} onMouseDown={(e) => e.preventDefault()} onClick={() => add(s.value)}>
              {str(s.label)}
            </div>
          ))}
        </div>
      ) : null}
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

/** Does a file match an `accept` list (`image/*,.pdf`)? */
export function acceptsFile(accept: string, file: { name: string; type: string }): boolean {
  const rules = accept
    .split(`,`)
    .map((r) => r.trim().toLowerCase())
    .filter(Boolean)
  if (rules.length === 0) return true
  const name = file.name.toLowerCase()
  const type = (file.type || ``).toLowerCase()
  return rules.some((r) => (r.startsWith(`.`) ? name.endsWith(r) : r.endsWith(`/*`) ? type.startsWith(r.slice(0, -1)) : type === r))
}

export function FileUploadNative({ node, props, rootProps, emit, scope, domId }: NativeProps) {
  const part = useParts(node, props)
  const ctx = useSurfaceContext()
  const id = useId()
  const external = arr<FileMeta>(props.files)
  const [files, setFiles] = useBoundState<FileMeta[]>(node, scope, `files`, external)
  const zoneRef = useRef<HTMLDivElement | null>(null)
  const inputRef = useRef<HTMLInputElement | null>(null)
  const f = useField({ node, domId, props, value: files, focusRef: zoneRef })
  const disabled = bool(props.disabled) || Boolean(f.form?.disabled)
  const multiple = bool(props.multiple)
  const accept = str(props.accept)
  const maxSize = props.maxSize === undefined ? Infinity : num(props.maxSize, Infinity)
  const [drag, setDrag] = useState(false)
  const [busy, setBusy] = useState(false)
  const [refused, setRefused] = useState<string[]>([])
  const name = str(props.name, node.id)
  const size = (n: number | undefined) => (n === undefined ? `` : formatFileSize(n, ctx.locale))
  const take = (list: FileList | File[] | null) => {
    if (!list || disabled) return
    const picked = Array.from(list).slice(0, multiple ? undefined : 1)
    const ok: File[] = []
    const errors: string[] = []
    for (const file of picked) {
      if (!acceptsFile(accept, file)) errors.push(phrase(ctx, `unsupportedFileNamed`, { name: file.name }, () => `${file.name}: ${ctx.t(`unsupportedFile`)}`))
      else if (file.size > maxSize) errors.push(phrase(ctx, `fileTooLargeNamed`, { name: file.name }, () => `${file.name}: ${ctx.t(`fileTooLarge`)}`))
      else ok.push(file)
    }
    setRefused(errors)
    if (ok.length === 0) return
    const meta = ok.map((file) => ({ name: file.name, size: file.size, type: file.type }))
    const next = multiple ? [...files.filter((x) => !meta.some((m) => m.name === x.name)), ...meta] : meta
    setFiles(next)
    f.change(next)
    void emit(`upload`, { files: meta })
    const r = ctx.host.onUpload?.(ok, { nodeId: domId, name })
    if (r && typeof (r as Promise<void>).then === `function`) {
      setBusy(true)
      void (r as Promise<void>).finally(() => setBusy(false))
    }
  }
  const remove = (fileName: string) => {
    const next = files.filter((x) => x.name !== fileName)
    setFiles(next)
    f.change(next)
    void emit(`remove`, { name: fileName })
  }
  const onDrag = (e: DragEvent, over: boolean) => {
    e.preventDefault()
    if (disabled) return
    setDrag(over)
  }
  const hint = str(props.hint)
  return (
    <div {...(rootProps as Record<string, unknown>)}>
      <TextPart part={part(`label`)} text={props.label} as="label" htmlFor={id} />
      <div
        ref={zoneRef}
        {...(part(`dropzone`, drag && `dragover`, disabled && `disabled`, f.invalid && `invalid`) as Record<string, string>)}
        role="button"
        tabIndex={disabled ? -1 : 0}
        aria-disabled={disabled || undefined}
        aria-busy={busy || undefined}
        aria-describedby={joinIds(hint && `${id}-hint`, refused.length > 0 && `${id}-refused`, f.describedBy)}
        aria-label={str(props.label) || ctx.t(`dropFiles`)}
        data-dragover={drag ? `` : undefined}
        onClick={() => !disabled && inputRef.current?.click()}
        onKeyDown={(e) => {
          if (disabled) return
          if (e.key === `Enter` || e.key === ` `) {
            e.preventDefault()
            inputRef.current?.click()
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
        <span {...(part(`icon`) as Record<string, string>)}>
          <BuiltinIcon slot="FileUpload.icon" />
        </span>
        <span {...(part(`title`) as Record<string, string>)}>{ctx.t(`dropFiles`)}</span>
        {hint ? (
          <span {...(part(`hint`) as Record<string, string>)} id={`${id}-hint`}>
            {hint}
          </span>
        ) : null}
        {/* Round 2 §7: the Browse button inside the zone (an outline small
            Button's look; the zone itself is the control). */}
        <span {...(part(`browse`) as Record<string, string>)} className={`${(part(`browse`) as { className: string }).className} ${partClass(`Button`, `root`)}`} data-r-variant="outline" data-r-size="sm" aria-hidden="true">
          {ctx.t(`browse`)}
        </span>
        <input ref={inputRef} id={id} type="file" className="xui-sr-only" tabIndex={-1} accept={accept || undefined} multiple={multiple} disabled={disabled} name={name} onChange={(e) => {
          take(e.target.files)
          e.target.value = ``
        }} />
      </div>
      {files.length ? (
        <ul className="xui-file-list">
          {files.map((file, i) => (
            <li key={file.name} {...(part.at(`file`, i) as Record<string, string>)}>
              <span {...(part.at(`fileIcon`, i) as Record<string, string>)}>
                <BuiltinIcon slot="FileUpload.file" />
              </span>
              <span {...(part.at(`fileName`, i) as Record<string, string>)}>{file.url ? <a href={file.url} target="_blank" rel="noreferrer">{file.name}</a> : file.name}</span>
              {file.size !== undefined ? <span {...(part.at(`fileMeta`, i) as Record<string, string>)}>{size(file.size)}</span> : null}
              {disabled ? null : (
                <button type="button" {...(part.at(`remove`, i) as Record<string, string>)} aria-label={phrase(ctx, `removeItem`, { name: file.name }, () => `${ctx.t(`remove`)} ${file.name}`)} onClick={() => remove(file.name)}>
                  <BuiltinIcon slot="FileUpload.remove" />
                </button>
              )}
            </li>
          ))}
        </ul>
      ) : null}
      {refused.length ? (
        <div {...(part(`error`) as Record<string, string>)} id={`${id}-refused`} role="alert">
          {refused.map((m) => (
            <span key={m} className="xui-field-error">
              {m}
            </span>
          ))}
        </div>
      ) : null}
      <FieldErrors part={part} field={f} />
    </div>
  )
}
