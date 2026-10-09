import {
  conceptIcon,
  Separator,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@exp/ui"

// EXP-456 / EXP-851: the row the two SLID-IN sidebar panels wear — settings
// and the list nav. ONE component, so the back affordance can't drift into
// two shapes, and the whole row is the target. EXP-870: the panels sit UNDER
// the fixed team-picker header now, beside the compact rail; the row keeps
// its `h-10` (the desktop's 40px back row matches it).

const UiBackIcon = conceptIcon(`ui-back`)

export function SidebarBackRow({
  label,
  onBack,
}: {
  /** Where back goes — "Settings", the board's name, "Inbox", "Support",
   *  "Agent", "Reviews". */
  label: string
  /** EXP-1246: absent on a list SCREEN (the md+ Inbox page) — the row is the
   *  list's plain title there, the compact rail beside it is the way out. */
  onBack?: () => void
}) {
  if (!onBack) {
    return (
      <>
        <SidebarHeader className="p-2">
          <div className="flex h-10 items-center px-2">
            <span className="min-w-0 truncate text-sm font-semibold">
              {label}
            </span>
          </div>
        </SidebarHeader>
        <Separator />
      </>
    )
  }
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
