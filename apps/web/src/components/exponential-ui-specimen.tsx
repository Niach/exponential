import { useMemo } from "react"
import { CORE_CATALOG_ID, DEFAULT_THEME_ID } from "@exponential-at/ui"
import type { NestedNode } from "@exponential-at/ui"
import specimens from "@exponential-at/ui/fixtures/specimens.json"
import { ExponentialSurface, useSurface, type HostPlugin } from "@exponential-at/ui-react"
import { exponentialUiIcons } from "@/lib/exponential-ui-icons"

// VAPP-93: one specimens.json entry painted alone on the page ground (the
// ui.exponential.at web shot of that component, view id = specimen id).
type Specimen = { id: string; component: string | null; title: string; node: NestedNode }

const SPECIMENS = (specimens as unknown as { specimens: Specimen[] }).specimens

export const SPECIMEN_IDS: readonly string[] = SPECIMENS.map((s) => s.id)

export function isSpecimenId(v: unknown): v is string {
  return typeof v === `string` && SPECIMEN_IDS.includes(v)
}

type Props = { id: string; theme?: string; mode?: `light` | `dark`; width?: number; rtl?: boolean }

export function ExponentialUiSpecimen(props: Props) {
  // Keyed so switching ids builds a fresh surface.
  return <SpecimenSurface key={props.id} {...props} />
}

function SpecimenSurface({ id, theme, mode, width, rtl }: Props) {
  const specimen = SPECIMENS.find((s) => s.id === id)!
  const surface = useSurface({ surfaceId: id, catalogId: CORE_CATALOG_ID, initial: specimen.node })
  const host = useMemo<HostPlugin>(
    () => ({
      icons: exponentialUiIcons,
      onAction: (e) => {
        console.info(`[exponential-ui] action`, e.name, e.componentId, e.context, e.payload)
        return new Promise((r) => setTimeout(r, 150))
      },
      onInput: (e) =>
        new Promise<void>((resolve) =>
          setTimeout(() => {
            if (e.path) surface.setData(e.path, e.value)
            resolve()
          }, 150)
        ),
    }),
    [surface]
  )
  return (
    <div className="min-h-dvh w-full overflow-y-auto bg-background">
      <div data-testid="exponential-ui-specimen" data-specimen={id}>
        <ExponentialSurface surface={surface} host={host} theme={theme ?? DEFAULT_THEME_ID} mode={mode ?? `dark`} direction={rtl ? `rtl` : `ltr`} width={width} />
      </div>
    </div>
  )
}
