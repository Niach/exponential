import * as React from "react"

import { AgentMark, agentLabel } from "./agent-picker"
import { cn } from "./cn"
import { Combobox } from "./combobox"
import { HoverCard, HoverCardContent, HoverCardTrigger } from "./hover-card"
import { Meter, type MeterTone } from "./meter"
import type { PickerOption } from "./picker-option"
import { useIsMobile } from "./use-mobile"

// EXP-872: ONE account picker per platform (web here, desktop
// `coding_selects::account_picker`, iOS `AccountPickerMenu`, Android
// `AccountPickerPill`). It REPLACES the agent picker + the account picker on
// every launch surface: there is no separate agent pick any more — the list
// is every signed-in login the machine reports, across both agents, and
// picking one implies its agent (`lib/accounts/account-option.ts`
// `flattenAccounts` owns the rules; this file only draws them).
//
// The row and the chip read the same way: the agent's brand mark + the
// login's EMAIL. Never the profile name, never the word "default" — the
// device default is simply the first row. A dead credential rides as a muted
// `hint` beside the email (the app passes `healthBadgeLabel`).
//
// EXP-992: hovering a row on a POINTER platform shows a very small preview
// right of the item — three tiny bars, `5h` / `week` / `<model>` labelled
// left of each — off the option's `limits` (fractions 0..1). On touch the
// same three bars sit inline under the email in the sheet row.

/** The three usage fractions a login carries (`AccountOption.limits`). */
export interface AccountLimits {
  fiveHour: number
  week: number
  model?: { label: string; used: number }
}

/** What the picker draws per login — the app maps `AccountOption` into it. */
export interface AccountPickerOption {
  /** The stable identity of the row (`${agent}:${profileId}`). */
  key: string
  agent: string
  email: string
  /** A muted note beside the email: `Needs re-login`, `Signed out`. */
  hint?: string
  limits?: AccountLimits
  disabled?: boolean
}

/** The bar labels, byte-identical ×4. The model bar is labelled by the
 *  window's own name, lower-cased (`fable`). */
export const ACCOUNT_LIMIT_LABELS = { fiveHour: `5h`, week: `week` } as const

/** EXP-909's thresholds as fractions: ≥ 0.75 warning, ≥ 0.95 danger. */
export function limitTone(used: number): MeterTone {
  if (used >= 0.95) return `danger`
  if (used >= 0.75) return `warning`
  return `normal`
}

/** The bars in order: 5h, week, then the model window when there is one. */
export function accountLimitBars(
  limits: AccountLimits
): { key: string; label: string; used: number }[] {
  const bars: { key: string; label: string; used: number }[] = [
    { key: `fiveHour`, label: ACCOUNT_LIMIT_LABELS.fiveHour, used: limits.fiveHour },
    { key: `week`, label: ACCOUNT_LIMIT_LABELS.week, used: limits.week },
  ]
  if (limits.model) {
    bars.push({
      key: `model`,
      label: limits.model.label.toLowerCase(),
      used: limits.model.used,
    })
  }
  return bars
}

/** The compact three-bar block: a 10px label column, a 4px meter per row. */
export function AccountLimitBars({
  limits,
  className,
}: {
  limits: AccountLimits
  className?: string
}) {
  return (
    <div
      data-slot="account-limit-bars"
      className={cn(`flex w-full flex-col gap-1`, className)}
    >
      {accountLimitBars(limits).map((bar) => (
        <div key={bar.key} className="flex items-center gap-1.5">
          <span className="w-8 shrink-0 truncate text-[10px] leading-none text-muted-foreground">
            {bar.label}
          </span>
          <Meter
            value={Math.round(Math.min(1, Math.max(0, bar.used)) * 100)}
            tone={limitTone(bar.used)}
            className="h-1 min-w-8 flex-1"
          />
        </div>
      ))}
    </div>
  )
}

/** Brand mark + email (+ hint): what the chip and every row say. */
export function AccountOptionLabel({
  option,
  markClassName,
  className,
}: {
  option: Pick<AccountPickerOption, `agent` | `email` | `hint`>
  markClassName?: string
  className?: string
}) {
  return (
    <span
      data-slot="account-option-label"
      className={cn(`inline-flex min-w-0 items-center gap-1.5`, className)}
      title={`${agentLabel(option.agent)} · ${option.email}`}
    >
      <AgentMark agent={option.agent} className={cn(`size-3.5`, markClassName)} />
      <span className="min-w-0 truncate">{option.email}</span>
      {option.hint && (
        <span className="shrink-0 text-xs text-muted-foreground">{option.hint}</span>
      )}
    </span>
  )
}

/** One menu row: the label, plus the limits preview — a hover card to the
 *  right on a pointer platform, the bars inline on touch. */
function AccountOptionRow({
  option,
  touch,
}: {
  option: AccountPickerOption
  touch: boolean
}) {
  const body = (
    <span className="flex min-w-0 flex-1 flex-col gap-1 py-0.5 text-sm">
      <AccountOptionLabel option={option} />
      {touch && option.limits && <AccountLimitBars limits={option.limits} />}
    </span>
  )
  if (touch || !option.limits) return body
  return (
    <HoverCard openDelay={150} closeDelay={80}>
      <HoverCardTrigger asChild>{body}</HoverCardTrigger>
      <HoverCardContent
        side="right"
        align="center"
        sideOffset={10}
        data-slot="account-limits-preview"
        className="w-36 p-2"
      >
        <AccountLimitBars limits={option.limits} />
      </HoverCardContent>
    </HoverCard>
  )
}

/** THE account picker. `inline` = one word of the composer's muted options
 *  line (collapses to plain text with a single login), `row` = the glass
 *  form ladder's picker row (`mobileTitle` leading, the pick trailing). */
export function AccountPicker({
  value,
  options,
  onChange,
  variant = `inline`,
  mobileTitle = `Account`,
  disabled = false,
  align,
  width = `md`,
  className,
  "data-testid": testId,
}: {
  /** The picked option's `key`, or null while none is. */
  value: string | null
  options: readonly AccountPickerOption[]
  onChange: (key: string) => void
  variant?: `inline` | `row`
  mobileTitle?: string
  disabled?: boolean
  align?: `start` | `end`
  width?: `sm` | `md` | `lg`
  className?: string
  "data-testid"?: string
}) {
  const touch = useIsMobile()
  const byKey = React.useMemo(
    () => new Map(options.map((option) => [option.key, option])),
    [options]
  )
  const pickerOptions = React.useMemo<PickerOption[]>(
    () =>
      options.map((option) => ({
        value: option.key,
        label: <AccountOptionLabel option={option} />,
        keywords: [option.email, agentLabel(option.agent)],
        disabled: option.disabled,
      })),
    [options]
  )
  return (
    <Combobox
      triggerVariant={variant}
      searchable={false}
      mobileTitle={mobileTitle}
      value={value}
      options={pickerOptions}
      align={align}
      width={width}
      disabled={disabled}
      className={className}
      data-testid={testId}
      onChange={(next) => {
        if (next !== null) onChange(next)
      }}
      renderOption={(option) => {
        const account = byKey.get(option.value)
        return account ? (
          <AccountOptionRow option={account} touch={touch} />
        ) : (
          <span className="min-w-0 flex-1 truncate text-sm">{option.label}</span>
        )
      }}
    />
  )
}
