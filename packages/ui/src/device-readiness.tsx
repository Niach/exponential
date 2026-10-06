import deviceDoctorSpec from "@exp/domain-contract/fixtures/device-doctor.json"

import { cn } from "./cn"
import { GlassSectionHeader, GlassToggleRow, ListRow, SETTINGS_LIST_CLASS } from "./glass-rows"
import { conceptIcon } from "./icons.generated"
import { Pill } from "./pill"

// EXP-1196/1218/1219: THE device readiness block, one spec ×5 —
// `packages/domain-contract/fixtures/device-doctor.json`. The device writes
// the report (`devices.doctor`: items with key/group/state/detail/action);
// this file only draws it: a band per group (the `tag` trailing, muted), one
// flat row per item (state glyph, label, the device's detail, at most ONE
// trailing pill), the computer_use item as a switch row, permission rows
// indented under it and hidden while it is off. No subtitles, no footers.
// Vocabulary the spec does not name (a newer device) is skipped, never shown
// raw.

export interface DeviceReadinessItem {
  key: string
  group: string
  parent?: string | null
  state: string
  detail?: string | null
  action?: string | null
}
export interface DeviceReadinessDoctor {
  checkedAt: string
  items: readonly DeviceReadinessItem[]
}

type Spec = typeof deviceDoctorSpec
export type DeviceReadinessTone = `success` | `warning` | `muted` | `destructive`
export type DeviceReadinessGlyph = `check` | `alert` | `dash` | `x`

const GROUPS = deviceDoctorSpec.groups
const LABELS = deviceDoctorSpec.labels as Record<string, string>
const STATES = deviceDoctorSpec.states as Record<
  string,
  { glyph: DeviceReadinessGlyph; tone: DeviceReadinessTone }
>
const ACTIONS = deviceDoctorSpec.actions as Record<string, { label: string; remote: boolean }>
export const DEVICE_READINESS_COPY: Spec[`copy`] = deviceDoctorSpec.copy

/** The item that IS the switch row. */
export const COMPUTER_USE_KEY = `computer_use`

export interface DeviceReadinessRowModel {
  key: string
  label: string
  state: string
  glyph: DeviceReadinessGlyph
  tone: DeviceReadinessTone
  detail: string | null
  /** The offered action (null = no pill: none, unknown, or not remote). */
  action: string | null
  actionLabel: string | null
  primary: boolean
  /** A permission row under its parent. */
  child: boolean
  /** The computer_use switch row: label + switch, no glyph, no detail. */
  isSwitch: boolean
}
export interface DeviceReadinessGroupModel {
  key: string
  label: string
  tag: string | null
  rows: DeviceReadinessRowModel[]
}

export interface DeviceReadinessModelOptions {
  /** Another device: only `remote: true` actions are offered. */
  remote: boolean
  /** Only these item keys (the composer's single failing row). */
  only?: readonly string[]
  /** Only rows that need something (`action` / `error`). */
  problemsOnly?: boolean
  /** The switch's live value (optimistic) — overrides the item's state for
   *  hiding the permission rows. */
  computerUseOn?: boolean
}

const isProblem = (state: string) => state === `action` || state === `error`

/** The block as data, fixture-locked by `device-readiness.test.tsx`. */
export function deviceReadinessModel(
  doctor: DeviceReadinessDoctor,
  options: DeviceReadinessModelOptions
): DeviceReadinessGroupModel[] {
  const known = doctor.items.filter(
    (item) =>
      LABELS[item.key] !== undefined &&
      STATES[item.state] !== undefined &&
      GROUPS.some((group) => group.key === item.group)
  )
  const parentOn = (key: string) => {
    if (key === COMPUTER_USE_KEY && options.computerUseOn !== undefined) {
      return options.computerUseOn
    }
    const parent = known.find((item) => item.key === key)
    return parent !== undefined && parent.state !== `off`
  }
  let visible = known.filter((item) => !item.parent || parentOn(item.parent))
  if (options.only) {
    const only = options.only
    visible = visible.filter((item) => only.includes(item.key))
  }
  if (options.problemsOnly) {
    visible = visible.filter((item) => isProblem(item.state))
  }
  const offered = (item: DeviceReadinessItem) => {
    const spec = item.action ? ACTIONS[item.action] : undefined
    if (!spec || item.key === COMPUTER_USE_KEY) return null
    return options.remote && !spec.remote ? null : item.action!
  }
  // The primary pill: the action of the first action/error row that offers
  // one in THIS rendering.
  const primaryKey =
    visible.find((item) => isProblem(item.state) && offered(item) !== null)?.key ?? null
  return GROUPS.map((group) => ({
    key: group.key,
    label: group.label,
    tag: group.tag,
    rows: visible
      .filter((item) => item.group === group.key)
      .map((item) => {
        const state = STATES[item.state]!
        const action = offered(item)
        const isSwitch = item.key === COMPUTER_USE_KEY
        return {
          key: item.key,
          label: LABELS[item.key]!,
          state: item.state,
          glyph: state.glyph,
          tone: state.tone,
          detail: isSwitch ? null : (item.detail ?? null),
          action,
          actionLabel: action ? ACTIONS[action]!.label : null,
          primary: action !== null && item.key === primaryKey,
          child: Boolean(item.parent),
          isSwitch,
        }
      }),
  })).filter((group) => group.rows.length > 0)
}

/** The row the composer shows when `agent` cannot start on the device: Git
 *  when Git is the failure, else that agent's own row; null = no doctor row
 *  explains it (the caller keeps its old sentence). */
export function deviceReadinessBlocker(
  doctor: DeviceReadinessDoctor | null | undefined,
  agent: string | null | undefined
): string | null {
  if (!doctor) return null
  const git = doctor.items.find((item) => item.key === `git`)
  if (git && git.state !== `ok`) return `git`
  if (!agent) return null
  const row = doctor.items.find((item) => item.key === agent)
  if (row && row.state !== `ok` && LABELS[row.key] !== undefined) return row.key
  return null
}

/** Whether the report says `agent` can run there (Git ok + the agent ok). */
export function deviceReadinessRunnable(
  doctor: DeviceReadinessDoctor,
  agent: string
): boolean {
  const state = (key: string) => doctor.items.find((item) => item.key === key)?.state
  return state(`git`) === `ok` && state(agent) === `ok`
}

const GLYPHS: Record<DeviceReadinessGlyph, ReturnType<typeof conceptIcon>> = {
  check: conceptIcon(`ui-check`),
  alert: conceptIcon(`ui-warning`),
  dash: conceptIcon(`ui-minus`),
  x: conceptIcon(`ui-close`),
}
const GLYPH_TONE: Record<DeviceReadinessTone, string> = {
  success: `text-emerald-400`,
  warning: `text-amber-400`,
  muted: `text-muted-foreground`,
  destructive: `text-destructive`,
}
const DETAIL_TONE: Record<string, string> = {
  action: `text-amber-400`,
  error: `text-destructive`,
}

export interface DeviceReadinessProps {
  doctor: DeviceReadinessDoctor | null | undefined
  /** Another device than the one rendering (every web surface). */
  remote: boolean
  onAction: (itemKey: string, action: string) => void
  /** The computer_use switch (launch_defaults.computerUse). Absent = the
   *  switch mirrors the report, read-only. */
  computerUse?: {
    checked: boolean
    onCheckedChange: (checked: boolean) => void
    disabled?: boolean
  }
  /** Single-row mode: only these item keys, no bands. */
  only?: readonly string[]
  /** Collapsed: only rows that need something. */
  problemsOnly?: boolean
  /** Disables every pill (an action in flight). */
  busy?: boolean
  className?: string
}

export function DeviceReadiness({
  doctor,
  remote,
  onAction,
  computerUse,
  only,
  problemsOnly,
  busy,
  className,
}: DeviceReadinessProps) {
  if (!doctor) return null
  const groups = deviceReadinessModel(doctor, {
    remote,
    only,
    problemsOnly,
    computerUseOn: computerUse?.checked,
  })
  if (groups.length === 0) return null
  const bands = !only && !problemsOnly
  const renderRow = (row: DeviceReadinessRowModel) => {
    if (row.isSwitch) {
      return (
        <GlassToggleRow
          key={row.key}
          id={`device-readiness-${row.key}`}
          label={row.label}
          checked={computerUse?.checked ?? row.state !== `off`}
          onCheckedChange={computerUse?.onCheckedChange ?? (() => {})}
          disabled={computerUse ? computerUse.disabled : true}
          className="px-3"
        />
      )
    }
    const Glyph = GLYPHS[row.glyph]
    return (
      <ListRow
        key={row.key}
        data-item={row.key}
        data-state={row.state}
        className={cn(`gap-2.5`, row.child && `pl-9`)}
      >
        <Glyph aria-hidden className={cn(`size-4 shrink-0`, GLYPH_TONE[row.tone])} />
        {/* The label keeps its width; the device's detail takes what is
            left and is the one that truncates. */}
        <span className="shrink-0 text-sm text-foreground">{row.label}</span>
        <span
          className={cn(
            `min-w-0 flex-1 truncate text-right text-xs`,
            DETAIL_TONE[row.state] ?? `text-muted-foreground`
          )}
        >
          {row.detail}
        </span>
        {row.action && row.actionLabel && (
          <Pill
            mode="action"
            size="sm"
            primary={row.primary}
            disabled={busy}
            data-action={row.action}
            onClick={() => onAction(row.key, row.action!)}
          >
            {row.actionLabel}
          </Pill>
        )}
      </ListRow>
    )
  }
  if (!bands) {
    return (
      <div data-slot="device-readiness" className={cn(SETTINGS_LIST_CLASS, className)}>
        {groups.flatMap((group) => group.rows).map(renderRow)}
      </div>
    )
  }
  return (
    <div data-slot="device-readiness" className={cn(`flex flex-col gap-4`, className)}>
      {groups.map((group) => (
        <div key={group.key} data-group={group.key}>
          <GlassSectionHeader
            label={group.label}
            trailing={
              group.tag ? (
                <span className="text-xs text-muted-foreground">{group.tag}</span>
              ) : undefined
            }
          />
          <div className={SETTINGS_LIST_CLASS}>{group.rows.map(renderRow)}</div>
        </div>
      ))}
    </div>
  )
}
