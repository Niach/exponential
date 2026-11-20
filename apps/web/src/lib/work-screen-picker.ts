// NEW-59 / #EXP-1514: the Work screen picker row of device settings
// (work-screen.json). Its items are the contract's options for the device's
// doctor: the reported displays, and while its `agent_space` row is `ok`
// (`spaceOffered`) Auto reads `Auto · Agent space` and `Agent space only`
// follows it. Values are the contract's picker keys (#FEED-164); the stored
// value stays the bare id (`workScreenPickedId`).
import {
  spaceOffered,
  workScreenOptions,
  workScreenPickerKey,
  type WorkScreenDisplay,
  type WorkScreenDoctor,
} from "@exp/domain-contract/work-screen"

export interface WorkScreenPickerDoctor extends WorkScreenDoctor {
  displays?: readonly WorkScreenDisplay[] | null
}

export function workScreenPickerItems(
  doctor: WorkScreenPickerDoctor | null | undefined,
  stored: string | null | undefined
): { value: string; label: string }[] {
  return workScreenOptions(doctor?.displays, stored, spaceOffered(doctor)).map((option) => ({
    value: workScreenPickerKey(option.value),
    label: option.label,
  }))
}
