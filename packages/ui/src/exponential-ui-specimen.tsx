import { useMemo, type ReactNode } from "react"
import { CORE_CATALOG_ID, DEFAULT_THEME_ID, type NestedNode } from "@exponential_app/eui"
import specimens from "@exponential_app/eui/fixtures/specimens.json"
import { ExponentialSurface, FormContext, useSurface, type FormContextValue } from "@exponential_app/eui-react"
import { planCardHost } from "./plan-card"

// VAPP-162: ONE Exponential UI specimen (`@exponential_app/eui`
// fixtures/specimens.json, the same JSON ui.exponential.at and the
// `exponential-ui-*` shots paint) through THE React renderer, so a
// styleguide entry that shows an eUI piece can never drift from the SDK.
// `cases` picks specimen cases by index (each = its caption + the node);
// no transport (planCardHost: the app's icon map, events reach nobody).

type Specimen = { id: string; component: string | null; title: string; node: NestedNode }

const SPECIMENS = (specimens as unknown as { specimens: Specimen[] }).specimens

/** The specimen ids (`exponential-ui-qr-code`, …). */
export const EXPONENTIAL_UI_SPECIMEN_IDS: readonly string[] = SPECIMENS.map((s) => s.id)

/** The specimen's root with only the chosen cases (all when unset). */
export function exponentialUiSpecimenNode(id: string, cases?: readonly number[]): NestedNode {
  const specimen = SPECIMENS.find((s) => s.id === id)
  if (!specimen) throw new Error(`no Exponential UI specimen ${id}`)
  if (!cases) return specimen.node
  const children = specimen.node.children ?? []
  return { ...specimen.node, children: cases.map((i) => children[i]).filter((c): c is NestedNode => c !== undefined) }
}

export function ExponentialUiSpecimen({
  id,
  cases,
  mode = `dark`,
}: {
  id: string
  cases?: readonly number[]
  mode?: `light` | `dark`
}) {
  const node = useMemo(() => exponentialUiSpecimenNode(id, cases), [id, cases])
  const surface = useSurface({ surfaceId: `sg-${id}`, catalogId: CORE_CATALOG_ID, initial: node })
  const host = useMemo(() => planCardHost(), [])
  return (
    <div data-slot="exponential-ui-specimen" data-specimen={id}>
      <ExponentialSurface surface={surface} host={host} theme={DEFAULT_THEME_ID} mode={mode} />
    </div>
  )
}

/** Round 9 §6.6: one Input per check severity, its message showing (the
 *  `attempts` of a refused submit reveal a field's checks at first paint). */
const FIELD_STATES: NestedNode = {
  id: `field-states`,
  component: `Stack`,
  props: { gap: `md` },
  children: [
    [`email`, `Email`, `ada@`, `/error`],
    [`password`, `Password`, `hunter2`, `/warning`],
    [`nickname`, `Nickname`, `ada`, `/info`],
  ].map(([id, label, value, path]) => ({ id: id!, component: `Input`, props: { label, name: id, value, validateOn: `submit`, checks: [{ condition: { "@path": path } }] } })),
} as NestedNode
const FIELD_STATE_DATA = {
  error: { valid: false, severity: `error`, message: `Not an email` },
  warning: { valid: false, severity: `warning`, message: `Weak: add a few more characters` },
  info: { valid: false, severity: `info`, message: `Shown on your profile` },
}
const SHOWN_CHECKS: FormContextValue = { register: () => () => {}, submit: () => {}, changed: () => {}, disabled: false, busy: false, attempts: 1 }

export function ExponentialUiFieldStates({ mode = `dark` }: { mode?: `light` | `dark` }) {
  const surface = useSurface({ surfaceId: `sg-field-states`, catalogId: CORE_CATALOG_ID, initial: FIELD_STATES, data: FIELD_STATE_DATA })
  const host = useMemo(() => planCardHost(), [])
  return (
    <div data-slot="exponential-ui-specimen" data-specimen="field-states">
      <FormContext.Provider value={SHOWN_CHECKS}>
        <ExponentialSurface surface={surface} host={host} theme={DEFAULT_THEME_ID} mode={mode} />
      </FormContext.Provider>
    </div>
  )
}

/** The app's piece beside its Exponential UI twin (the styleguide entries
 *  of the pieces the app already draws): `children` = the app's, then the
 *  named eUI component painted from its specimen. */
export function ExponentialUiTwin({
  name,
  id,
  cases,
  children,
}: {
  name: string
  id: string
  cases?: readonly number[]
  children?: ReactNode
}) {
  return (
    <div data-slot="exponential-ui-twin" className="flex flex-col gap-6">
      {children}
      <div className="flex flex-col gap-2">
        <span className="text-xs font-medium text-muted-foreground">{`genUI · ${name}`}</span>
        <ExponentialUiSpecimen id={id} cases={cases} />
      </div>
    </div>
  )
}

/** EXP-1604: an inline Exponential UI tree (a styleguide entry's own
 *  specimens, e.g. apps/styleguide/src/entries/motion.specimens.json)
 *  through THE React renderer, with its data model; `inlineOverlays` paints
 *  an open Dialog / Drawer in place (EXP-1673). `frame` (px, with
 *  `inlineOverlays`) gives the overlay a box of that height to hang in, its
 *  backdrop over the box (VAPP-262: what the gpui painter shows for a
 *  surface with a viewport of that height). */
export function ExponentialUiTree({ id, node, data, mode = `dark`, formFactor, inlineOverlays, frame }: { id: string; node: NestedNode; data?: Record<string, unknown>; mode?: `light` | `dark`; formFactor?: `phone` | `wide`; inlineOverlays?: boolean; frame?: number }) {
  const surface = useSurface({ surfaceId: `sg-${id}`, catalogId: CORE_CATALOG_ID, initial: node, data })
  const host = useMemo(() => planCardHost(), [])
  const scope = `[data-specimen="${id}"] [data-xui-overlays="inline"]`
  return (
    <div data-slot="exponential-ui-tree" data-specimen={id} style={frame ? { position: `relative`, height: frame, overflow: `hidden`, transform: `translateZ(0)` } : undefined}>
      {frame ? (
        <style>{`${scope} :is(.xui-Dialog-overlay,.xui-Drawer-overlay){display:block}${scope} .xui-Drawer-content{position:fixed;inset:auto 0 0 0;width:auto;max-height:85%}${scope} .xui-Dialog-content{position:fixed;inset:auto;left:50%;top:50%;transform:translate(-50%,-50%);width:calc(100% - 32px)}`}</style>
      ) : null}
      <ExponentialSurface surface={surface} host={host} theme={DEFAULT_THEME_ID} mode={mode} formFactor={formFactor} inlineOverlays={inlineOverlays} />
    </div>
  )
}
