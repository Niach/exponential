import type { ComponentProps } from "react"
import { CHECKBOX_CLASSES } from "@exponential_app/eui/web/primitives"
import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"

export * from "@exponential_app/eui-react/primitives/checkbox"

const CheckIcon = conceptIcon(`ui-check`)
const MinusIcon = conceptIcon(`ui-minus`)

/** THE checkbox's look as a passive mark: the same box (CHECKBOX_CLASSES,
 *  16/1/4) for a row that is itself the control (a touch selection cell, a
 *  multi-select choice), so no row ever draws a round selection glyph. */
export function CheckboxMark({
  checked,
  className,
  ...props
}: ComponentProps<`span`> & { checked: boolean | `indeterminate` }) {
  const Glyph = checked === `indeterminate` ? MinusIcon : CheckIcon
  return (
    <span
      aria-hidden
      data-slot="checkbox-mark"
      data-state={checked === `indeterminate` ? `indeterminate` : checked ? `checked` : `unchecked`}
      className={cn(CHECKBOX_CLASSES.root, CHECKBOX_CLASSES.indicator, `data-[state=indeterminate]:border-primary data-[state=indeterminate]:bg-primary data-[state=indeterminate]:text-primary-foreground`, className)}
      {...props}
    >
      {checked && <Glyph className={CHECKBOX_CLASSES.icon} />}
    </span>
  )
}
