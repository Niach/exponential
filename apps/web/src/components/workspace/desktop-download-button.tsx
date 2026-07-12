import { Download } from "lucide-react"
import { desktopDownloadHref } from "@/lib/desktop-download"
import { SidebarMenuButton, SidebarMenuItem } from "@/components/ui/sidebar"

// Modest bottom-of-sidebar "get the desktop app" entry (EXP-68 — the Claude
// web UI pattern; replaces the old dismissible card on the Agents page). The
// href targets the visitor's OS asset directly and falls back to the GitHub
// releases page on mobile/unknown platforms.
export function DesktopDownloadButton() {
  const href =
    typeof navigator === `undefined`
      ? desktopDownloadHref(``)
      : desktopDownloadHref(navigator.userAgent, navigator.maxTouchPoints)
  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild className="text-muted-foreground">
        <a href={href} target="_blank" rel="noreferrer">
          <Download className="h-4 w-4" />
          <span>Download the desktop app</span>
        </a>
      </SidebarMenuButton>
    </SidebarMenuItem>
  )
}
