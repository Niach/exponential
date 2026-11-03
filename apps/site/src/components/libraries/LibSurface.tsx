/* A build-time eUI surface for the libraries' diagrams: EuiSurface's twin
   with a host that carries LIB_ICONS. Used without a client directive only,
   so it ships HTML and CSS, no JS; the dark/light switch is the surface's
   CSS variables (eui-styles). */
import { ExponentialSurface, useSurface } from "@exponential_app/eui-react"
import type { HostPlugin } from "@exponential_app/eui-react"
import type { NestedNode } from "@exponential_app/eui"
import { LIB_ICONS } from "./icons.ts"

const HOST: HostPlugin = { icons: LIB_ICONS, onInput: () => Promise.resolve(), onAction: () => Promise.resolve() }

export default function LibSurface({ id, tree }: { id: string; tree: NestedNode }) {
  const surface = useSurface({ surfaceId: id, initial: tree })
  return <ExponentialSurface id={`xui-${id}`} surface={surface} theme="exponential" mode="dark" host={HOST} />
}
