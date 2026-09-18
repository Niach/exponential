import { useCallback, useMemo, useState } from "react"
import { ChevronDown } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Button, Combobox, type PickerOption } from "@exp/ui"

// A searchable branch picker for one connected repository (EXP-462/469,
// generalized in EXP-712 so the repo settings' default-branch pin and the
// board form's branch field share one control). Branch names load from
// GitHub on first open. `value` is the EFFECTIVE branch the host acts on and
// always renders even when GitHub no longer has it (a pin deleted upstream) —
// otherwise the menu couldn't show what the row is set to, let alone offer
// the way back. `repoDefault` is the branch that means "follow the repo":
// it carries the `default` tag and picking it reports `null`.
//
// EXP-941: the shared `Combobox` — the lazy GitHub load rides its `loading`
// and `error` slots, the `default` tag is the option `hint`.
export function BranchCombobox({
  repositoryId,
  value,
  repoDefault,
  onPick,
  disabled,
  ariaLabel,
  size = `default`,
  className,
  align = `start`,
  rowLabel,
}: {
  repositoryId: string
  value: string
  repoDefault: string
  onPick: (branch: string | null) => void
  disabled?: boolean
  ariaLabel: string
  size?: `sm` | `default`
  className?: string
  align?: `start` | `end`
  /** EXP-862: render as a PICKER ROW of a glass group instead of a button —
   *  the label leads the row, the branch sits at the trailing edge (the
   *  board form). Unset keeps the standalone button (the repo settings
   *  row's branch badge). */
  rowLabel?: string
}) {
  const [open, setOpen] = useState(false)
  const [branches, setBranches] = useState<string[] | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)

  const load = useCallback(async () => {
    setLoading(true)
    setLoadError(null)
    try {
      const { branches: names } = await trpc.repositories.listBranches.query({
        repositoryId,
      })
      setBranches(names)
    } catch (err) {
      setLoadError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [repositoryId])

  const names = useMemo(
    () =>
      branches && !branches.includes(value) ? [value, ...branches] : branches,
    [branches, value]
  )
  const options = useMemo<PickerOption[]>(
    () =>
      (names ?? []).map((name) => ({
        value: name,
        label: name,
        hint: name === repoDefault ? `default` : undefined,
      })),
    [names, repoDefault]
  )

  return (
    <Combobox
      options={options}
      value={value}
      onChange={(name) => {
        if (!name || name === value) return
        onPick(name === repoDefault ? null : name)
      }}
      open={open}
      onOpenChange={(next) => {
        setOpen(next)
        if (next && branches === null && !loading) void load()
      }}
      disabled={disabled}
      // Nothing has loaded and nothing failed = the very first frame of the
      // lazy fetch; showing the empty-list copy there would read as "this repo
      // has no branches".
      loading={loading || (names === null && loadError === null)}
      error={
        loadError ? (
          <Button
            variant="ghost"
            size="sm"
            className="w-full justify-start rounded-none text-destructive hover:text-destructive"
            onClick={() => void load()}
          >
            Couldn&rsquo;t load branches — retry
          </Button>
        ) : undefined
      }
      width="md"
      align={align}
      mobileTitle="Branch"
      placeholder="Search branches…"
      emptyText="No branches found."
      renderOption={(option) => (
        <>
          <span className="min-w-0 flex-1 truncate font-mono text-xs">
            {option.label}
          </span>
          {option.hint !== undefined && (
            <span className="ml-2 shrink-0 text-xs text-muted-foreground">
              {option.hint}
            </span>
          )}
        </>
      )}
      renderTrigger={() =>
        rowLabel ? (
          <button
            type="button"
            disabled={disabled}
            aria-label={ariaLabel}
            className={`flex w-full items-center gap-3 px-4 py-3 text-left transition-colors duration-fast hover:bg-glass-active/50 disabled:pointer-events-none disabled:opacity-50 ${className ?? ``}`}
          >
            <span className="shrink-0 text-sm text-foreground">{rowLabel}</span>
            <span className="ml-auto min-w-0 truncate font-mono text-sm text-foreground/70">
              {value}
            </span>
            <ChevronDown className="size-3.5 shrink-0 text-foreground/50" />
          </button>
        ) : (
          <Button
            type="button"
            variant="outline"
            size={size}
            disabled={disabled}
            className={className}
            aria-label={ariaLabel}
          >
            <span className="min-w-0 truncate">{value}</span>
            <ChevronDown className="h-3 w-3 shrink-0 text-muted-foreground" />
          </Button>
        )
      }
    />
  )
}
