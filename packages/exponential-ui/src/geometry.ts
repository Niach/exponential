// VAPP-92: the MEASURE contract a painter (built-in or a host's override,
// VAPP-91) must keep for a control: its box comes from the theme's tokens
// through the recipe of the part that IS the control (Button/root,
// Switch/track, Checkbox/box…), never from the painter. The fixture
// `control-geometry.json` records every built-in theme × control × size,
// and `checkGeometry` is what the conformance suite runs an override through.

import { resolveRecipe } from "./theme"
import type { ModeName, ResolvedStyle, ResolvedTheme } from "./theme-types"

/** The part whose recipe sizes each control. */
export const CONTROL_PARTS: Record<string, string> = {
  Button: `root`,
  Toggle: `root`,
  ToggleGroup: `item`,
  Input: `field`,
  Textarea: `field`,
  Select: `trigger`,
  DatePicker: `trigger`,
  Checkbox: `box`,
  Radio: `item`,
  Switch: `track`,
  Slider: `thumb`,
  Avatar: `root`,
  Icon: `root`,
  Spinner: `root`,
  Ring: `root`,
  Tabs: `tab`,
  // Round 1: the new fields keep the input height contract.
  NumberField: `field`,
  ChipInput: `field`,
  TimePicker: `trigger`,
  DateRangePicker: `trigger`,
}

export const GEOMETRY_KEYS = [`width`, `height`, `minWidth`, `minHeight`, `paddingHorizontal`, `paddingVertical`, `padding`, `gap`, `borderWidth`, `borderRadius`] as const
export type GeometryKey = (typeof GEOMETRY_KEYS)[number]
export type ControlGeometry = Partial<Record<GeometryKey, number>>

/** The numeric box of a control for a theme (mode-independent: lengths do
 *  not change with the mode). */
export function controlGeometry(theme: ResolvedTheme, component: string, props: Record<string, unknown> = {}, states: readonly string[] = []): ControlGeometry {
  const part = CONTROL_PARTS[component] ?? `root`
  const style: ResolvedStyle = resolveRecipe(theme, { component, part, props, states }, `light` as ModeName)
  const out: ControlGeometry = {}
  for (const key of GEOMETRY_KEYS) {
    const v = style[key]
    if (typeof v === `number`) out[key] = v
  }
  return out
}

/** What a painter reports after measuring the control it drew. */
export interface MeasuredBox {
  width?: number
  height?: number
  paddingHorizontal?: number
  paddingVertical?: number
  borderWidth?: number
  borderRadius?: number
}

/** A host's replacement painter for one native kind (the VAPP-91 extension
 *  API): it may draw anything, but `measure` must return the theme's box. */
export interface PainterOverride {
  component: string
  measure(theme: ResolvedTheme, props: Record<string, unknown>, states?: readonly string[]): MeasuredBox
}

export interface GeometryIssue {
  key: string
  expected: number
  actual: number | undefined
}

/** Every geometry key the theme fixes that the measured box misses or
 *  changes (a key the theme leaves open is the painter's). */
export function checkGeometry(expected: ControlGeometry, measured: MeasuredBox, tolerance = 0.5): GeometryIssue[] {
  const issues: GeometryIssue[] = []
  const probe = (key: GeometryKey, actual: number | undefined) => {
    const want = expected[key]
    if (want === undefined) return
    if (actual === undefined || Math.abs(actual - want) > tolerance) issues.push({ key, expected: want, actual })
  }
  probe(`width`, measured.width)
  // A control with a minimum height (Textarea) grows from it: the recipe's
  // `height` is its one-line base, so only the floor is checked (VAPP-91).
  if (expected.minHeight === undefined) probe(`height`, measured.height)
  probe(`paddingHorizontal`, measured.paddingHorizontal)
  probe(`paddingVertical`, measured.paddingVertical)
  probe(`borderWidth`, measured.borderWidth)
  probe(`borderRadius`, measured.borderRadius)
  if (expected.minHeight !== undefined && (measured.height === undefined || measured.height + tolerance < expected.minHeight))
    issues.push({ key: `minHeight`, expected: expected.minHeight, actual: measured.height })
  return issues
}

/** Run an override through every case the fixture holds for its kind. */
export function verifyPainterGeometry(
  override: PainterOverride,
  theme: ResolvedTheme,
  cases: readonly { props: Record<string, unknown>; states?: readonly string[] }[]
): { name: string; issues: GeometryIssue[] }[] {
  return cases
    .map((c) => ({
      name: Object.entries(c.props).map(([k, v]) => `${k}=${String(v)}`).join(`,`) || `default`,
      issues: checkGeometry(controlGeometry(theme, override.component, c.props, c.states), override.measure(theme, c.props, c.states)),
    }))
    .filter((r) => r.issues.length > 0)
}
