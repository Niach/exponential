import { conceptIcon } from "@/lib/icons.generated"
import { Separator } from "@/components/ui/separator"
import {
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"

// EXP-456 / EXP-851: the row the two SLID-IN sidebar panels wear — settings
// and the list nav. ONE component, so the back affordance can't drift into
// two shapes: `h-10` matches the team-switcher row, so the top edge never
// jumps mid-slide, and the whole row is the target.

const UiBackIcon = conceptIcon(`ui-back`)

export function SidebarBackRow({
  label,
  onBack,
}: {
  /** Where back goes — "Settings", the board's name, "Inbox", "Support",
   *  "Agent", "Reviews". */
  label: string
  onBack: () => void
}) {
  return (
    <>
      <SidebarHeader className="p-2">
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              onClick={onBack}
              aria-label={`Back to ${label}`}
              className="h-10"
            >
              <UiBackIcon className="h-4 w-4" />
              <span className="min-w-0 truncate text-sm font-semibold">
                {label}
              </span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>

      <Separator />
    </>
  )
}
