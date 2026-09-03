import { useCallback, useState } from "react"
import { Check, ChevronDown, LoaderCircle } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Button } from "@/components/ui/button"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import {
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
} from "@/components/mobile-popover"

// A searchable branch picker for one connected repository (EXP-462/469,
// generalized in EXP-712 so the repo settings' default-branch pin and the
// board form's branch field share one control). Branch names load from
// GitHub on first open. `value` is the EFFECTIVE branch the host acts on and
// always renders even when GitHub no longer has it (a pin deleted upstream) —
// otherwise the menu couldn't show what the row is set to, let alone offer
// the way back. `repoDefault` is the branch that means "follow the repo":
// it carries the `default` tag and picking it reports `null`.
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

  const names =
    branches && !branches.includes(value) ? [value, ...branches] : branches

  return (
    <MobilePopover
      open={open}
      onOpenChange={(next) => {
        setOpen(next)
        if (next && branches === null && !loading) void load()
      }}
    >
      <MobilePopoverTrigger asChild>
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
      </MobilePopoverTrigger>
      <MobilePopoverContent
        className="w-[16rem] p-0"
        align={align}
        mobileTitle="Branch"
      >
        <Command>
          <CommandInput placeholder="Search branches…" />
          <CommandList>
            {loading && (
              <div className="flex items-center gap-2 px-3 py-2 text-sm text-muted-foreground">
                <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                Loading branches…
              </div>
            )}
            {loadError && (
              <Button
                variant="ghost"
                size="sm"
                className="w-full justify-start rounded-none text-destructive hover:text-destructive"
                onClick={() => void load()}
              >
                Couldn&rsquo;t load branches — retry
              </Button>
            )}
            {names && (
              <>
                <CommandEmpty>No branches found.</CommandEmpty>
                <CommandGroup>
                  {names.map((name) => (
                    <CommandItem
                      key={name}
                      value={name}
                      onSelect={() => {
                        setOpen(false)
                        if (name === value) return
                        onPick(name === repoDefault ? null : name)
                      }}
                    >
                      <Check
                        className={`h-3.5 w-3.5 shrink-0 ${name === value ? `` : `invisible`}`}
                      />
                      <span className="min-w-0 flex-1 truncate font-mono text-xs">
                        {name}
                      </span>
                      {name === repoDefault && (
                        <span className="ml-2 text-xs text-muted-foreground">
                          default
                        </span>
                      )}
                    </CommandItem>
                  ))}
                </CommandGroup>
              </>
            )}
          </CommandList>
        </Command>
      </MobilePopoverContent>
    </MobilePopover>
  )
}
