import type { PinKind } from "@exp/db-schema/domain"
import { conceptIcon } from "@/lib/icons.generated"
import { usePinToggle } from "@/hooks/use-pins"
import { IconTooltip } from "@/components/icon-tooltip"
import { Button } from "@/components/ui/button"
import { DropdownMenuItem } from "@/components/ui/dropdown-menu"

// EXP-778: the small pin toggle beside a title — issue detail, the session
// header, the action editor. Pinned rows land in the sidebar's Pinned group
// on every client; the glyph is the shared `ui-pin` / `ui-unpin` concept.
const UiPinIcon = conceptIcon(`ui-pin`)
const UiUnpinIcon = conceptIcon(`ui-unpin`)

export function pinLabel(pinned: boolean) {
  return pinned ? `Unpin` : `Pin to sidebar`
}

export function PinToggleButton({
  teamId,
  kind,
  targetId,
  variant = `glass`,
  size = `icon-sm`,
  className,
}: {
  teamId: string | undefined
  kind: PinKind
  targetId: string | undefined
  variant?: `glass` | `ghost`
  size?: `icon-sm` | `icon`
  className?: string
}) {
  const { pinned, toggle, busy } = usePinToggle(teamId, kind, targetId)
  const label = pinLabel(pinned)
  return (
    <IconTooltip label={label}>
      <Button
        variant={variant}
        size={size}
        aria-label={label}
        aria-pressed={pinned}
        disabled={busy || !teamId || !targetId}
        onClick={toggle}
        className={className}
      >
        {pinned ? <UiUnpinIcon /> : <UiPinIcon />}
      </Button>
    </IconTooltip>
  )
}

/** The same toggle as a row of an overflow menu (the phone headers fold
 *  every action into one `…`, EXP-687). */
export function PinToggleMenuItem({
  teamId,
  kind,
  targetId,
}: {
  teamId: string | undefined
  kind: PinKind
  targetId: string | undefined
}) {
  const { pinned, toggle } = usePinToggle(teamId, kind, targetId)
  return (
    <DropdownMenuItem onSelect={toggle}>
      {pinned ? (
        <UiUnpinIcon className="h-4 w-4" />
      ) : (
        <UiPinIcon className="h-4 w-4" />
      )}
      {pinLabel(pinned)}
    </DropdownMenuItem>
  )
}
