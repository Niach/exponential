import { conceptIcon } from "@/lib/icons.generated"
import { Button } from "@/components/ui/button"
import { useSidebarIfMounted } from "@/components/ui/sidebar"

// EXP-870: the md+ detail headers carry no back control while the sidebar is
// up — the compact rail and the list nav's back row are right there. Cmd+B
// hides the whole sidebar (shadcn offcanvas), and then the list is one click
// away again only through this: a 32px ghost chevron (the one back control,
// desktop `controls::back_button`) that goes to the ORIGIN list
// (`originListNavigation`). Renders nothing while the sidebar is open.

const UiBackIcon = conceptIcon(`ui-back`)

export function DetailBackChevron({
  onBack,
  label = `Back`,
  className,
}: {
  onBack: () => void
  label?: string
  className?: string
}) {
  const sidebar = useSidebarIfMounted()
  if (!sidebar || sidebar.open || sidebar.isMobile) return null
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      className={className ?? `shrink-0`}
      aria-label={label}
      onClick={onBack}
      data-testid="detail-back-chevron"
    >
      <UiBackIcon />
    </Button>
  )
}
