/* The component page's studio: a big LIVE render (React renderer,
   client-only island) with theme, mode, direction and width switches, a
   props switcher generated from the docs props table, and tabs for the A2UI
   JSON and the embed code of every platform, all following the current
   state. Everything but the surface is prerendered markup. */
import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode, type RefObject } from "react"
import { DocsCode } from "@exp/site-shell"
import type { ComponentDoc, Specimen, ThemesDoc } from "../lib/catalog"
import { useModePick } from "../lib/scheme"
import { controlFor, flatten, ownProps, pretty, specimenSubject, surfaceMessages, type Control, type Nested, type PropDoc } from "./a2ui"
import { groundVars, type Backgrounds } from "./backgrounds"
import { EMBED_TARGETS, embedCode, type EmbedTarget } from "./embed"
import { SurfacePlaceholder, useRuntime } from "./Island"

/* Overlays that open as MODALS lock the page's scroll and trap focus, so on
   a docs page they start closed (the shots show them open; `open` is one
   toggle away). Non-modal overlays start open. */
const NON_MODAL_OVERLAYS = new Set([`Popover`, `Tooltip`])

type Mode = `light` | `dark`
type Dir = `ltr` | `rtl`
export const WIDTHS = [`fit`, 390, 768, 1280] as const
type Width = (typeof WIDTHS)[number]
type Tab = `preview` | `json` | EmbedTarget

const SURFACE_ID = `preview`

export function initialProps(doc: ComponentDoc, subject: Nested | null): Record<string, unknown> {
  const props: Record<string, unknown> = { ...ownProps(doc.example as Record<string, unknown>), ...(subject?.props ?? {}) }
  if (doc.props.some((p) => p.name === `open`)) props.open = doc.group !== `overlay` || NON_MODAL_OVERLAYS.has(doc.name)
  return props
}

/* Overlays a small render may show open: they neither trap focus nor lock
   the page (the render's box contains their fixed layer). */
const THUMB_OPEN = new Set([`Popover`, `Tooltip`, `Toast`])

/** Small renders (index cards, gallery) open only the harmless overlays. */
export function thumbProps(doc: ComponentDoc, subject: Nested | null): Record<string, unknown> {
  const props = initialProps(doc, subject)
  if (doc.group === `overlay` && `open` in props) props.open = THUMB_OPEN.has(doc.name)
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

export const subjectOf = (doc: ComponentDoc, specimen: Specimen | undefined) => (specimen ? specimenSubject(specimen.node as unknown as Nested, doc.name) : null)

/** The element's content width, live (0 before the first measure). */
function useWidth<T extends HTMLElement>(): [RefObject<T | null>, number] {
  const ref = useRef<T>(null)
  const [w, setW] = useState(0)
  useEffect(() => {
    const el = ref.current
    if (!el) return
    const ro = new ResizeObserver(([e]) => setW(e!.contentRect.width))
    ro.observe(el)
    return () => ro.disconnect()
  }, [])
  return [ref, w]
}

export function ComponentStudio({ doc, specimen, themes, backgrounds }: { doc: ComponentDoc; specimen: Specimen | undefined; themes: ThemesDoc[`themes`]; backgrounds: Backgrounds }) {
  const subject = useMemo(() => subjectOf(doc, specimen), [specimen, doc])
  const initial = useMemo(() => initialProps(doc, subject), [doc, subject])
  const [props, setProps] = useState<Record<string, unknown>>(initial)
  const [theme, setTheme] = useState(`exponential`)
  const { mode, cls: modeClass, setMode } = useModePick()
  const [dir, setDir] = useState<Dir>(`ltr`)
  const [width, setWidth] = useState<Width>(`fit`)
  const [tab, setTab] = useState<Tab>(`preview`)
  const [generation, setGeneration] = useState(0)
  const runtime = useRuntime()
  const [stageRef, stageWidth] = useWidth<HTMLDivElement>()

  const components = useMemo(() => flatten(studioTree(doc, subject, props, caseAlign(specimen))), [doc, subject, props, specimen])
  const messages = useMemo(() => surfaceMessages(SURFACE_ID, components), [components])
  const json = useMemo(() => pretty(messages), [messages])

  const set = (name: string, value: unknown) =>
    setProps((cur) => {
      const next = { ...cur }
      if (value === undefined) delete next[name]
      else next[name] = value
      return next
    })

  const controls = doc.props.map((p) => ({ prop: p as PropDoc, control: controlFor(p as PropDoc, props[p.name] ?? initial[p.name]) })).filter((c): c is { prop: PropDoc; control: Control } => c.control !== null)

  const tabs: { id: Tab; label: string }[] = [{ id: `preview`, label: `Preview` }, { id: `json`, label: `A2UI JSON` }, ...EMBED_TARGETS.map((t) => ({ id: t.id, label: t.label }))]
  const scale = typeof width === `number` && stageWidth > 0 ? Math.min(1, stageWidth / width) : 1
  const surfaceWidth = typeof width === `number` ? width : undefined

  return (
    <div className="sdk-studio">
      <div className="sdk-studio-bar" role="group" aria-label="Preview settings">
        <Seg label="Theme" options={themes.map((t) => ({ value: t.id, label: t.name }))} value={theme} onChange={setTheme} />
        <Seg label="Mode" options={[{ value: `light`, label: `Light` }, { value: `dark`, label: `Dark` }]} value={mode} onChange={(m) => setMode(m as Mode)} />
        <Seg label="Direction" options={[{ value: `ltr`, label: `LTR` }, { value: `rtl`, label: `RTL` }]} value={dir} onChange={(d) => setDir(d as Dir)} />
        <Seg label="Width" options={WIDTHS.map((w) => ({ value: String(w), label: w === `fit` ? `Fit` : `${w}`, aria: w === `fit` ? `Fit the column` : `${w} px` }))} value={String(width)} onChange={(w) => setWidth(w === `fit` ? `fit` : (Number(w) as Width))} />
        <span className="sdk-grow" />
        <button
          type="button"
          className="btn btn-ghost btn-sm"
          onClick={() => {
            setProps(initial)
            setGeneration((g) => g + 1)
          }}
        >
          Reset
        </button>
      </div>

      <div
        ref={stageRef}
        className={`sdk-stage is-${modeClass} is-hero${doc.group === `overlay` ? ` is-overlay` : ``}${typeof width === `number` ? ` is-sized` : ``}`}
        data-theme={theme}
        style={groundVars(backgrounds[theme])}
      >
        <div className="sdk-stage-frame" style={typeof width === `number` ? { width, zoom: scale } : undefined}>
          {runtime ? (
            <runtime.LiveSurface surfaceId={SURFACE_ID} domId={`studio-${doc.specimenId}`} messages={messages} theme={theme} mode={mode} direction={dir} width={surfaceWidth} icons={runtime.icons} />
          ) : (
            <SurfacePlaceholder minHeight={180} />
          )}
        </div>
        {typeof width === `number` && (
          <span className="sdk-stage-size" aria-hidden="true">
            {width} px{scale < 1 ? ` · ${Math.round(scale * 100)}%` : ``}
          </span>
        )}
      </div>

      <div className="sdk-tabs" role="tablist" aria-label="Preview, JSON and embed code" onKeyDown={(e) => tabKeys(e, tabs, tab, setTab)}>
        {tabs.map((t) => (
          <button key={t.id} id={`tab-${t.id}`} type="button" role="tab" aria-selected={tab === t.id} aria-controls={`panel-${t.id}`} tabIndex={tab === t.id ? 0 : -1} onClick={() => setTab(t.id)}>
            {t.label}
          </button>
        ))}
      </div>

      <div id="panel-preview" role="tabpanel" aria-labelledby="tab-preview" hidden={tab !== `preview`} className="sdk-panel">
        {controls.length > 0 ? (
          <div className="sdk-controls">
            {controls.map(({ prop, control }) => (
              <PropControl key={`${prop.name}-${generation}`} prop={prop} control={control} value={props[prop.name]} onChange={(v) => set(prop.name, v)} />
            ))}
          </div>
        ) : (
          <p className="sdk-note">No props to switch.</p>
        )}
      </div>
      <div id="panel-json" role="tabpanel" aria-labelledby="tab-json" hidden={tab !== `json`} className="sdk-panel">
        <p className="sdk-note">
          <code>createSurface</code> + <code>updateComponents</code>, A2UI v0.9.
        </p>
        <DocsCode language="json">{json}</DocsCode>
      </div>
      {EMBED_TARGETS.map((t) => (
        <div key={t.id} id={`panel-${t.id}`} role="tabpanel" aria-labelledby={`tab-${t.id}`} hidden={tab !== t.id} className="sdk-panel">
          <p className="sdk-note">
            This surface through a <code>MemoryTransport</code>. Install and transports: <a href={t.guide}>{t.label} guide</a>.
          </p>
          {tab === t.id && <DocsCode language={t.language}>{embedCode(t.id, { messages, surfaceId: SURFACE_ID, theme, mode, direction: dir, width: surfaceWidth })}</DocsCode>}
        </div>
      ))}
    </div>
  )
}

function tabKeys(e: KeyboardEvent, tabs: { id: Tab }[], current: Tab, set: (t: Tab) => void) {
  const i = tabs.findIndex((t) => t.id === current)
  let next = -1
  if (e.key === `ArrowRight`) next = (i + 1) % tabs.length
  else if (e.key === `ArrowLeft`) next = (i - 1 + tabs.length) % tabs.length
  else if (e.key === `Home`) next = 0
  else if (e.key === `End`) next = tabs.length - 1
  if (next < 0) return
  e.preventDefault()
  set(tabs[next]!.id)
  document.getElementById(`tab-${tabs[next]!.id}`)?.focus()
}

function Seg({ label, options, value, onChange }: { label: string; options: { value: string; label: string; aria?: string }[]; value: string; onChange: (v: string) => void }) {
  return (
    <div className="sdk-seg" role="group" aria-label={label}>
      {options.map((o) => (
        <button key={o.value} type="button" aria-pressed={value === o.value} aria-label={o.aria} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
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
        aria-invalid={bad || undefined}
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
