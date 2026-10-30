import type { JSX } from "solid-js"
import { ICON_DISC_CLASSES, type IconDiscTone } from "@exponential_app/eui/web/primitives"

import type { IconComponent } from "../host"
import { PaintIcon } from "../icons"
import { cn } from "./cn"

export { ICON_DISC_TONES } from "@exponential_app/eui/web/primitives"
export type { IconDiscTone } from "@exponential_app/eui/web/primitives"

/** lucide-react's own stroke width, its fallback for an undefined one. */
const LUCIDE_STROKE = 2
/** lucide-react's `focusable="false"` (not in Solid's SVG attribute types). */
const UNFOCUSABLE = { focusable: `false` } as JSX.SvgSVGAttributes<SVGSVGElement>

export function IconDisc(props: { icon: IconComponent; tone?: IconDiscTone; strokeWidth?: number; class?: string }) {
  return (
    <div data-slot="icon-disc" class={cn(ICON_DISC_CLASSES.root, ICON_DISC_CLASSES.tones[props.tone ?? `primary`], props.class)}>
      {/* Decorative: the disc always sits beside a title that names it. */}
      <PaintIcon icon={props.icon} class={ICON_DISC_CLASSES.icon} stroke-width={props.strokeWidth ?? LUCIDE_STROKE} aria-hidden="true" {...UNFOCUSABLE} />
    </div>
  )
}
