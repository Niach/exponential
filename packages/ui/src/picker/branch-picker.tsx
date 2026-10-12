import { useCallback, useMemo, useState, type ReactNode } from "react"

import { Button } from "../button"
import { actionErrorTitle, FEEDBACK_ERRORS, toastActionError } from "../feedback"
import { Picker, type PickerItem, type PickerSurfaceProps } from "./picker"
import type { PickerTriggerVariant } from "./picker-trigger"

// UI cleanup batch — the branch picker (was the web's `BranchCombobox`,
// EXP-462/469/712): a searchable pick out of ONE repository's branches,
// loaded on first open through `loadBranches` (the host's tRPC call).
//
// `value` is the EFFECTIVE branch the host acts on and always renders, even
// when the remote no longer has it (a pin deleted upstream) — otherwise the
// picker could not show what the row is set to, let alone offer the way
// back. `defaultBranch` is the branch that means "follow the repo": it wears
// the `default` hint, and picking it reports `null`.

export interface BranchPickerProps extends PickerSurfaceProps {
  value: string
  defaultBranch: string
  /** Fetches the branch names; called on the first open (and on retry). */
  loadBranches: () => Promise<string[]>
  /** A pick: the branch, or `null` for the default branch. Re-picking the
   *  current value reports nothing. */
  onPick: (branch: string | null) => void
  /** A bespoke trigger; omitted = the primitive's own in `triggerVariant`. */
  trigger?: ReactNode
  /** `row` leads with `label` (the board form); `field` is the standalone
   *  control. Defaults to `field`. */
  triggerVariant?: PickerTriggerVariant
  /** The row trigger's label and the trigger's accessible name. */
  label?: string
  disabled?: boolean
  className?: string
}

export function branchPickerItems(
  names: readonly string[],
  defaultBranch: string
): PickerItem[] {
  return names.map((name) => ({
    value: name,
    label: <span className="font-mono text-xs">{name}</span>,
    keywords: [name],
    hint: name === defaultBranch ? `default` : undefined,
  }))
}

export function BranchPicker({
  value,
  defaultBranch,
  loadBranches,
  onPick,
  trigger,
  triggerVariant = `field`,
  label = `Branch`,
  open: openProp,
  onOpenChange,
  width = `md`,
  ...props
}: BranchPickerProps) {
  const [uncontrolledOpen, setUncontrolledOpen] = useState(false)
  const open = openProp ?? uncontrolledOpen
  const [branches, setBranches] = useState<string[] | null>(null)
  const [loadFailed, setLoadFailed] = useState(false)
  const [loading, setLoading] = useState(false)

  // FEED-79: the list loads when the picker opens, so a failure is quiet:
  // the empty list + Retry (the next open tries again too). Only a failed
  // Retry tap, the person's own action, toasts.
  const load = useCallback(async (retried = false): Promise<void> => {
    setLoading(true)
    setLoadFailed(false)
    try {
      setBranches(await loadBranches())
    } catch (err) {
      setLoadFailed(true)
      if (retried) {
        toastActionError(actionErrorTitle(`load`, `branches`), {
          description: err instanceof Error && err.message.trim() ? err.message : undefined,
          retry: () => void load(true),
        })
      }
    } finally {
      setLoading(false)
    }
  }, [loadBranches])

  const names = useMemo(
    () =>
      branches && !branches.includes(value) ? [value, ...branches] : branches,
    [branches, value]
  )
  const items = useMemo(
    () => branchPickerItems(names ?? [], defaultBranch),
    [names, defaultBranch]
  )

  return (
    <Picker
      mode="single"
      items={items}
      value={value}
      onChange={(name) => {
        if (name === value) return
        onPick(name === defaultBranch ? null : name)
      }}
      open={open}
      onOpenChange={(next) => {
        if (openProp === undefined) setUncontrolledOpen(next)
        onOpenChange?.(next)
        if (next && branches === null && !loading) void load()
      }}
      search
      searchPlaceholder="Search branches…"
      emptyText="No branches found."
      // Nothing has loaded and nothing failed = the first frame of the lazy
      // fetch; the empty copy there would read as "this repo has no
      // branches".
      loading={loading || (open && names === null && !loadFailed)}
      error={
        loadFailed ? (
          <Button
            variant="ghost"
            size="sm"
            className="w-full justify-start rounded-none text-muted-foreground"
            onClick={() => void load(true)}
          >
            {FEEDBACK_ERRORS.retry}
          </Button>
        ) : undefined
      }
      mobileTitle={label}
      trigger={trigger}
      triggerVariant={triggerVariant}
      // Before the list loads, the trigger still says what the row is set to.
      triggerLabel={value}
      width={width}
      {...props}
    />
  )
}
