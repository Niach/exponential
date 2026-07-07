import { Download, Monitor } from "lucide-react"
import { Button } from "@/components/ui/button"

// Production desktop builds, always the latest GitHub release. The desktop IDE
// is the only client that runs coding sessions, so the Agents view offers the
// download inline instead of pointing at the marketing site.
const DESKTOP_DOWNLOADS: Array<{
  os: string
  caption: string
  href: string
}> = [
  {
    os: `macOS`,
    caption: `Apple silicon`,
    href: `https://github.com/Niach/exponential/releases/latest/download/exp-desktop-production.zip`,
  },
  {
    os: `Windows`,
    caption: `x86_64`,
    href: `https://github.com/Niach/exponential/releases/latest/download/Exponential-production-x86_64-windows.zip`,
  },
  {
    os: `Linux`,
    caption: `AppImage`,
    href: `https://github.com/Niach/exponential/releases/latest/download/Exponential-production-x86_64.AppImage`,
  },
]

// Compact "Get the desktop app" strip for the Agents view: three per-OS
// download buttons linking to the latest production release assets.
export function DesktopDownloadCard() {
  return (
    <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-3 rounded-md border border-border/50 bg-muted/30 px-4 py-3">
      <div className="min-w-0">
        <div className="flex items-center gap-2 text-sm font-medium">
          <Monitor className="size-4 text-muted-foreground" />
          Get the desktop app
        </div>
        <p className="mt-0.5 text-xs text-muted-foreground">
          Run coding sessions from the desktop IDE — they show up here to watch
          and steer.
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        {DESKTOP_DOWNLOADS.map(({ os, caption, href }) => (
          <Button key={os} variant="outline" size="sm" asChild>
            <a href={href} target="_blank" rel="noreferrer" download>
              <Download />
              {os}
              <span className="text-xs font-normal text-muted-foreground">
                {caption}
              </span>
            </a>
          </Button>
        ))}
      </div>
    </div>
  )
}
