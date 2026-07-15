import { useState } from "react"
import { Check, Copy, Globe } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"

// Share affordances for public boards. Copying the public link is NOT an
// owner privilege — any member may share a board that is already public —
// so these render wherever members are (board header, sidebar menu, and the
// owner-only settings dialog reuses the same link row).

export function buildPublicBoardUrl(
  workspaceSlug: string,
  projectSlug: string
) {
  return `${window.location.origin}/t/${workspaceSlug}/projects/${projectSlug}`
}

export function copyPublicBoardUrl(
  workspaceSlug: string,
  projectSlug: string
) {
  if (typeof navigator === `undefined` || !navigator.clipboard) return
  navigator.clipboard
    .writeText(buildPublicBoardUrl(workspaceSlug, projectSlug))
    .catch(() => {
      // Clipboard denied (permissions/insecure context) — nothing to do.
    })
}

// The public URL + copy button block, shared by the share popover and the
// owner settings dialog.
export function PublicBoardLinkRow({ url }: { url: string }) {
  const [copied, setCopied] = useState(false)

  return (
    <div className="flex items-center gap-2">
      <div className="flex h-9 min-w-0 flex-1 items-center rounded-md border px-3 text-xs text-muted-foreground">
        <span className="truncate">{url}</span>
      </div>
      <Button
        variant="outline"
        size="icon"
        className="h-9 w-9 shrink-0"
        title="Copy public link"
        onClick={() => {
          if (typeof navigator === `undefined` || !navigator.clipboard) return
          navigator.clipboard.writeText(url).then(
            () => {
              setCopied(true)
              setTimeout(() => setCopied(false), 1500)
            },
            () => {
              // Clipboard denied — no success state.
            }
          )
        }}
      >
        {copied ? (
          <Check className="h-3.5 w-3.5 text-primary" />
        ) : (
          <Copy className="h-3.5 w-3.5" />
        )}
      </Button>
    </div>
  )
}

// Board-header Share button for public projects: a small popover with the
// public URL and a reminder of what "public" means.
export function PublicBoardShareButton({
  workspaceSlug,
  projectSlug,
}: {
  workspaceSlug: string
  projectSlug: string
}) {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="xs" className="text-muted-foreground">
          <Globe className="size-3" />
          Share
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-96 space-y-3">
        <div>
          <p className="text-sm font-medium">Share this board</p>
          <p className="text-xs text-muted-foreground">
            This board is public — anyone with the link can view its issues.
          </p>
        </div>
        <PublicBoardLinkRow
          url={buildPublicBoardUrl(workspaceSlug, projectSlug)}
        />
      </PopoverContent>
    </Popover>
  )
}
