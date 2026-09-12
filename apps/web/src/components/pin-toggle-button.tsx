import type { PinKind } from "@exp/db-schema/domain"
import { conceptIcon } from "@/lib/icons.generated"
import { usePinToggle } from "@/hooks/use-pins"
import { useIsMobile } from "@/hooks/use-mobile"
import { IconTooltip } from "@/components/icon-tooltip"
import { Button } from "@/components/ui/button"
import { DropdownMenuItem } from "@/components/ui/dropdown-menu"
import { cn } from "@/lib/utils"

// EXP-778: the small pin toggle beside a title — issue detail, the session
// header, the action editor. Pinned rows land in the sidebar's Pinned group;
// the glyph is the shared `ui-pin` / `ui-unpin` concept.
// EXP-858: a pin only means something where a sidebar exists, so these
// controls render at `md` and up only — the phone layout (and the natives,
// which dropped their toggles entirely) has no Pinned group to land in. The
// synced rows still show up in the mobile board switcher's pinned section.
const UiPinIcon = conceptIcon(`ui-pin`)
const UiUnpinIcon = conceptIcon(`ui-unpin`)

export function pinLabel(pinned: boolean) {
  return pinned ? `Unpin` : `Pin to sidebar`
}

export function PinToggleButton({
  teamId,
  kind,
  targetId,
  // EXP-850 §10: GHOST is the default now — a pin toggle is a borderless
  // glyph on every surface (issue header, session header, action dialog); the
  // glass capsule is kept only for a caller that still asks for it.
  variant = `ghost`,
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
        className={cn(`hidden md:inline-flex`, className)}
      >
        {pinned ? <UiUnpinIcon /> : <UiPinIcon />}
      </Button>
    </IconTooltip>
  )
}

/** Whether `PinToggleMenuItem` renders anything at all (EXP-858: it does not
 *  on a phone viewport). A menu whose OTHER rows are conditional has to ask,
 *  or it hands a viewport a `⋯` trigger that opens an EMPTY popover. */
export function usePinToggleVisible(): boolean {
  return !useIsMobile()
}

/** The same toggle as a row of an overflow menu. EXP-858: dropped on a phone
 *  viewport, where there is no sidebar to pin into. */
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
  const visible = usePinToggleVisible()
  if (!visible) return null
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
