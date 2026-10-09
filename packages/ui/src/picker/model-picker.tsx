import type { ReactNode } from "react"

import { Picker, type PickerItem, type PickerSurfaceProps } from "./picker"
import type { PickerTriggerVariant } from "./picker-trigger"

// UI cleanup batch — the model and effort pickers: a short fixed list of
// `{ value, label }` choices (contract `codingModel` / the agent's effort
// values, plus the blank "CLI default" a host maps to its own sentinel). The
// composer's options line picks a model as one WORD of its sentence
// (`inline`); the forms and the composer's menu pick them as a row.

export interface ValueChoice {
  value: string
  label: string
  /** A muted trailing note. */
  hint?: ReactNode
  disabled?: boolean
}

interface ChoicePickerProps extends PickerSurfaceProps {
  value: string | null
  onChange: (value: string) => void
  /** A bespoke trigger; omitted = the primitive's own in `triggerVariant`. */
  trigger?: ReactNode
  triggerVariant?: PickerTriggerVariant
  triggerLabel?: string
  /** The sheet's title on a phone, and the row/inline trigger's label. */
  mobileTitle?: string
  disabled?: boolean
  className?: string
}

export function choicePickerItems(choices: readonly ValueChoice[]): PickerItem[] {
  return choices.map((choice) => ({
    value: choice.value,
    label: choice.label,
    hint: choice.hint,
    disabled: choice.disabled,
  }))
}

export interface ModelPickerProps extends ChoicePickerProps {
  models: readonly ValueChoice[]
}

export function ModelPicker({
  models,
  mobileTitle = `Model`,
  triggerVariant = `inline`,
  width = `sm`,
  ...props
}: ModelPickerProps) {
  return (
    <Picker
      mode="single"
      items={choicePickerItems(models)}
      mobileTitle={mobileTitle}
      triggerVariant={triggerVariant}
      width={width}
      {...props}
    />
  )
}

export interface EffortPickerProps extends ChoicePickerProps {
  efforts: readonly ValueChoice[]
}

export function EffortPicker({
  efforts,
  mobileTitle = `Effort`,
  triggerVariant = `row`,
  ...props
}: EffortPickerProps) {
  return (
    <Picker
      mode="single"
      items={choicePickerItems(efforts)}
      mobileTitle={mobileTitle}
      triggerVariant={triggerVariant}
      {...props}
    />
  )
}
