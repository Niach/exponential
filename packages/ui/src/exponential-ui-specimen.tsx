import { useMemo, type ReactNode } from "react"
import { CORE_CATALOG_ID, DEFAULT_THEME_ID, type NestedNode } from "@exponential_app/ui"
import specimens from "@exponential_app/ui/fixtures/specimens.json"
import { ExponentialSurface, useSurface } from "@exponential_app/ui-react"
import { planCardHost } from "./plan-card"

// VAPP-162: ONE Exponential UI specimen (`@exponential_app/ui`
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
        <span className="text-xs font-medium text-muted-foreground">{`Exponential UI · ${name}`}</span>
        <ExponentialUiSpecimen id={id} cases={cases} />
      </div>
    </div>
  )
}
