import { For, Show, type JSX } from "solid-js"
import { SEGMENTED_CONTROL_CLASSES } from "@exponential_app/eui/web/primitives"

import type { IconComponent } from "../host"
import { PaintIcon } from "../icons"
import { cn } from "./cn"
import { Tabs, TabsList, TabsTrigger } from "./tabs"

// The segmented capsule from an option array (React's segmented-control.tsx).
// The glass-group row (`embedded`) is the Exponential app's own arm, in
// @exp/ui; both SDK twins draw the floating capsule only.

interface SegmentedOption<TValue extends string> {
  value: TValue
  label: JSX.Element
  /** A leading glyph (a lucide-solid component), painted as lucide-react paints it. */
  icon?: IconComponent
  disabled?: boolean
}

interface SegmentedControlProps<TValue extends string> {
  value: TValue
  onValueChange: (value: TValue) => void
  options: readonly SegmentedOption<TValue>[]
  /** Span the container instead of the capsule's natural `w-fit`. */
  fill?: boolean
  /** Disables every segment. */
  disabled?: boolean
  /** Extra classes on the strip (the `TabsList`). */
  class?: string
}

function SegmentedControl<TValue extends string>(props: SegmentedControlProps<TValue>) {
  return (
    <Tabs data-slot="segmented-control" value={props.value} onValueChange={(next) => props.onValueChange(next as TValue)}>
      <TabsList class={cn(props.fill && SEGMENTED_CONTROL_CLASSES.fill, props.class)}>
        <For each={props.options}>
          {(option) => (
            <TabsTrigger value={option.value} disabled={props.disabled || option.disabled} class={SEGMENTED_CONTROL_CLASSES.trigger}>
              <Show when={option.icon}>{(glyph) => <PaintIcon icon={glyph()} aria-hidden="true" />}</Show>
              {option.label}
            </TabsTrigger>
          )}
        </For>
      </TabsList>
    </Tabs>
  )
}

export { SegmentedControl }
export type { SegmentedControlProps, SegmentedOption }
