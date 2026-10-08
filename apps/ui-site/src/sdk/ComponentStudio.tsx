/* The component page's studio: a LIVE render (React renderer, client-only
   island), a props switcher generated from the docs props table, and the
   A2UI JSON of the current switcher state. The switcher and the JSON are
   real prerendered markup; only the surface waits for hydration. */
import { useMemo, useState, type ReactNode } from "react"
import { DocsCode } from "@exp/site-shell"
import type { ComponentDoc, Specimen, ThemesDoc } from "../lib/catalog"
import { controlFor, flatten, ownProps, pretty, specimenSubject, surfaceMessages, updateComponents, type Control, type Nested, type PropDoc } from "./a2ui"
import type { Backgrounds } from "./backgrounds"
import { SurfacePlaceholder, useRuntime } from "./Island"

/* Overlays that open as MODALS lock the page's scroll and trap focus, so on
   a docs page they start closed (the shots show them open; `open` is one
   toggle away). Non-modal overlays start open. */
const NON_MODAL_OVERLAYS = new Set([`Popover`, `Tooltip`])

type Mode = `light` | `dark`

export function initialProps(doc: ComponentDoc, subject: Nested | null): Record<string, unknown> {
  const props: Record<string, unknown> = { ...ownProps(doc.example as Record<string, unknown>), ...(subject?.props ?? {}) }
  if (doc.props.some((p) => p.name === `open`)) props.open = doc.group !== `overlay` || NON_MODAL_OVERLAYS.has(doc.name)
  return props
}

/** The surface the studio renders: the component (id `example`, as in
 *  the docs) with the specimen's real children and slots, in a vertical
 *  Stack root aligned like the specimen's case (a root sizes like a page;
 *  a Select or a List wants a column to fill). */
export function studioTree(doc: ComponentDoc, subject: Nested | null, props: Record<string, unknown>, align = `stretch`): Nested {
  const example = doc.example as Record<string, unknown>
  const node: Nested = { id: `example`, component: doc.name, props }
  const style = (subject?.style ?? example.style) as Record<string, unknown> | undefined
  const on = (subject?.on ?? example.on) as Record<string, unknown> | undefined
  if (style) node.style = style
  if (on) node.on = on
  if (subject?.children?.length) node.children = subject.children
  if (subject?.slots) node.slots = subject.slots
  if (subject?.template) node.template = subject.template
  return { id: `root`, component: `Stack`, props: { direction: `vertical`, align }, children: [node] }
}

/** The specimen case's own alignment (the generator picks it per component). */
export const caseAlign = (specimen: Specimen | undefined): string => {
  const first = (specimen?.node as unknown as Nested | undefined)?.children?.[0]
  const align = first?.props?.align
  return typeof align === `string` ? align : `stretch`
}

export function ComponentStudio({ doc, specimen, themes, backgrounds }: { doc: ComponentDoc; specimen: Specimen | undefined; themes: ThemesDoc[`themes`]; backgrounds: Backgrounds }) {
  const subject = useMemo(() => (specimen ? specimenSubject(specimen.node as unknown as Nested, doc.name) : null), [specimen, doc.name])
  const initial = useMemo(() => initialProps(doc, subject), [doc, subject])
  const [props, setProps] = useState<Record<string, unknown>>(initial)
  const [theme, setTheme] = useState(`exponential`)
  const [mode, setMode] = useState<Mode>(`dark`)
  const [generation, setGeneration] = useState(0)
  const runtime = useRuntime()

  const components = useMemo(() => flatten(studioTree(doc, subject, props, caseAlign(specimen))), [doc, subject, props, specimen])
  const messages = useMemo(() => surfaceMessages(`preview`, components), [components])
  const json = useMemo(() => pretty(updateComponents(`preview`, components)), [components])

  const set = (name: string, value: unknown) =>
    setProps((cur) => {
      const next = { ...cur }
      if (value === undefined) delete next[name]
      else next[name] = value
      return next
    })

  const controls = doc.props.map((p) => ({ prop: p as PropDoc, control: controlFor(p as PropDoc, props[p.name] ?? initial[p.name]) })).filter((c): c is { prop: PropDoc; control: Control } => c.control !== null)

  return (
    <div className="sdk-studio">
      <div className="sdk-studio-bar">
        <div className="sdk-seg" role="group" aria-label="Theme">
          {themes.map((t) => (
            <button key={t.id} type="button" aria-pressed={theme === t.id} onClick={() => setTheme(t.id)}>
              {t.name}
            </button>
          ))}
        </div>
        <div className="sdk-seg" role="group" aria-label="Mode">
          {([`light`, `dark`] as const).map((m) => (
            <button key={m} type="button" aria-pressed={mode === m} onClick={() => setMode(m)}>
              {m === `light` ? `Light` : `Dark`}
            </button>
          ))}
        </div>
        <span className="sdk-grow" />
        <button type="button" className="btn btn-ghost btn-sm" onClick={() => {
            setProps(initial)
            setGeneration((g) => g + 1)
          }}>
          Reset
        </button>
      </div>
      <div className={`sdk-stage is-${mode}${doc.group === `overlay` ? ` is-overlay` : ``}`} data-theme={theme} style={{ background: backgrounds[theme]?.[mode] }}>
        {runtime ? <runtime.LiveSurface surfaceId="preview" domId={`studio-${doc.specimenId}`} messages={messages} theme={theme} mode={mode} icons={runtime.icons} /> : <SurfacePlaceholder minHeight={120} />}
      </div>
      {controls.length > 0 && (
        <div className="sdk-controls">
          {controls.map(({ prop, control }) => (
            <PropControl key={`${prop.name}-${generation}`} prop={prop} control={control} value={props[prop.name]} onChange={(v) => set(prop.name, v)} />
          ))}
        </div>
      )}
      <h3 className="sdk-subhead">A2UI JSON</h3>
      <p className="sdk-note">
        The current state as the agent sends it: one A2UI v0.9 <code>updateComponents</code> message (after <code>createSurface</code> with the core catalog id).
      </p>
      <DocsCode language="json">{json}</DocsCode>
    </div>
  )
}

function PropControl({ prop, control, value, onChange }: { prop: PropDoc; control: Control; value: unknown; onChange: (v: unknown) => void }) {
  const id = `prop-${prop.name}`
  const label = (
    <label className="sdk-control-label" htmlFor={id} title={prop.description}>
      {prop.name}
      {prop.required && <span className="sdk-req">required</span>}
    </label>
  )
  switch (control.kind) {
    case `enum`: {
      const current = (value ?? prop.default) as string | undefined
      const segmented = control.options.length <= 6 && control.options.join(``).length <= 44
      return (
        <div className="sdk-control is-wide">
          {label}
          {segmented ? (
            <div className="sdk-seg is-small" role="group" id={id} aria-label={prop.name}>
              {control.options.map((o) => (
                <button key={o} type="button" aria-pressed={current === o} onClick={() => onChange(o)}>
                  {o}
                </button>
              ))}
            </div>
          ) : (
            <select id={id} className="sdk-select" value={current ?? ``} onChange={(e) => onChange(e.target.value || undefined)}>
              {current === undefined && <option value="">(unset)</option>}
              {control.options.map((o) => (
                <option key={o} value={o}>
                  {o}
                </option>
              ))}
            </select>
          )}
        </div>
      )
    }
    case `boolean`: {
      const checked = Boolean(value ?? prop.default)
      return (
        <div className="sdk-control">
          {label}
          <button id={id} type="button" role="switch" aria-checked={checked} className="sdk-switch" onClick={() => onChange(!checked)}>
            <span />
          </button>
        </div>
      )
    }
    case `number`:
      return (
        <div className="sdk-control">
          {label}
          <input
            id={id}
            className="sdk-input"
            type="number"
            value={typeof value === `number` ? value : ``}
            placeholder={prop.default !== undefined ? String(prop.default) : ``}
            onChange={(e) => onChange(e.target.value === `` ? undefined : Number(e.target.value))}
          />
        </div>
      )
    case `text`:
      return (
        <div className={`sdk-control${control.multiline ? ` is-wide` : ``}`}>
          {label}
          {control.multiline ? (
            <textarea id={id} className="sdk-input sdk-mono" rows={3} value={typeof value === `string` ? value : ``} onChange={(e) => onChange(e.target.value || undefined)} />
          ) : (
            <input id={id} className="sdk-input" type="text" value={typeof value === `string` ? value : ``} placeholder={prop.default !== undefined ? String(prop.default) : prop.type} onChange={(e) => onChange(e.target.value || undefined)} />
          )}
        </div>
      )
    case `json`:
      return <JsonControl id={id} label={label} value={value} onChange={onChange} />
  }
}

function JsonControl({ id, label, value, onChange }: { id: string; label: ReactNode; value: unknown; onChange: (v: unknown) => void }) {
  const [text, setText] = useState(() => JSON.stringify(value))
  const [bad, setBad] = useState(false)
  return (
    <div className="sdk-control is-wide">
      {label}
      <textarea
        id={id}
        className={`sdk-input sdk-mono${bad ? ` is-bad` : ``}`}
        rows={Math.min(6, Math.max(2, Math.ceil(text.length / 70)))}
        spellCheck={false}
        value={text}
        onChange={(e) => {
          setText(e.target.value)
          try {
            onChange(e.target.value.trim() ? JSON.parse(e.target.value) : undefined)
            setBad(false)
          } catch {
            setBad(true)
          }
        }}
      />
    </div>
  )
}
