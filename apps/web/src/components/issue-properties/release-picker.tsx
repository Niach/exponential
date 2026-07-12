import { useMemo, useState } from "react"
import { useLiveQuery, eq } from "@tanstack/react-db"
import { Check, Rocket, X } from "lucide-react"
import { releaseCollection } from "@/lib/collections"
import { compareReleases } from "@/lib/releases"
import {
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
} from "@/components/mobile-popover"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from "@/components/ui/command"
import { Button } from "@/components/ui/button"
import type { Release } from "@/db/schema"

interface ReleasePickerProps {
  disabled?: boolean
  workspaceId: string
  selectedReleaseId: string | null
  onSelect: (releaseId: string | null) => void | Promise<void>
}

// Single-select release picker (issues.release_id is 1:N — an issue lives in
// at most one release). Mirrors the label picker's MobilePopover + Command
// structure; picking the current release is a no-op, "No release" clears.
export function ReleasePicker({
  disabled,
  workspaceId,
  selectedReleaseId,
  onSelect,
}: ReleasePickerProps) {
  const [open, setOpen] = useState(false)

  const { data: releaseRows } = useLiveQuery(
    (q) =>
      workspaceId
        ? q
            .from({ releases: releaseCollection })
            .where(({ releases }) => eq(releases.workspaceId, workspaceId))
        : undefined,
    [workspaceId]
  )

  const releases = useMemo(
    () => [...((releaseRows ?? []) as Release[])].sort(compareReleases),
    [releaseRows]
  )
  const selectedRelease =
    releases.find((release) => release.id === selectedReleaseId) ?? null

  const handlePick = (releaseId: string | null) => {
    setOpen(false)
    if (releaseId !== selectedReleaseId) {
      void onSelect(releaseId)
    }
  }

  return (
    <MobilePopover
      open={disabled ? false : open}
      onOpenChange={(o) => {
        if (disabled) return
        setOpen(o)
      }}
    >
      <MobilePopoverTrigger asChild>
        <Button
          variant="ghost"
          size="xs"
          className="text-muted-foreground"
          disabled={disabled}
        >
          <Rocket className="size-3" />
          {selectedRelease ? (
            <span className="max-w-[7.5rem] truncate">
              {selectedRelease.name}
            </span>
          ) : (
            `Release`
          )}
        </Button>
      </MobilePopoverTrigger>
      <MobilePopoverContent
        className="w-[14rem] p-0"
        align="start"
        mobileTitle="Release"
      >
        <Command>
          <CommandInput placeholder="Filter releases..." />
          <CommandList>
            <CommandEmpty>No releases found.</CommandEmpty>
            <CommandGroup>
              {releases.map((release) => (
                <CommandItem
                  key={release.id}
                  value={release.id}
                  keywords={[release.name]}
                  onSelect={() => handlePick(release.id)}
                  className="flex items-center gap-2"
                >
                  <Rocket className="size-3 shrink-0 text-muted-foreground" />
                  <span className="min-w-0 truncate text-sm">
                    {release.name}
                  </span>
                  {release.shippedAt !== null && (
                    <span className="shrink-0 text-xs text-muted-foreground">
                      shipped
                    </span>
                  )}
                  {release.id === selectedReleaseId && (
                    <Check className="ml-auto size-3.5 shrink-0" />
                  )}
                </CommandItem>
              ))}
            </CommandGroup>
            {selectedReleaseId && (
              <>
                <CommandSeparator />
                <CommandGroup>
                  <CommandItem
                    onSelect={() => handlePick(null)}
                    className="flex items-center gap-2"
                  >
                    <X className="size-3.5" />
                    <span className="text-sm">No release</span>
                  </CommandItem>
                </CommandGroup>
              </>
            )}
          </CommandList>
        </Command>
      </MobilePopoverContent>
    </MobilePopover>
  )
}
