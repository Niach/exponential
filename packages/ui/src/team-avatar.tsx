import { useEffect, useRef, useState } from "react"
import {
  TEAM_IMAGE_COPY,
  TEAM_IMAGE_MARK,
  TEAM_IMAGE_ROW,
  TEAM_IMAGE_TEST_IDS,
  TEAM_IMAGE_UPLOAD,
  teamMark,
} from "@exp/domain-contract/team-image"
import { Button } from "./button"
import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"
import { Pill } from "./pill"

// #EXP-1645 (fixtures/team-image.json) — the team mark, one component instead
// of a hand-rolled square in every switcher: the team's image in a rounded
// rectangle at a quarter of its side with no border and no fill (a transparent
// image stays transparent), else the group glyph in the surrounding text
// colour (iOS `TeamAvatar.swift`, Android `Avatars.kt`, desktop `team_avatar`).
const TeamGlyph = conceptIcon(`ui-team`)
const RemoveIcon = conceptIcon(`ui-delete`)

export function TeamAvatar({
  iconUrl,
  size = 20,
  className,
}: {
  /** `teams.icon_url`. */
  iconUrl: string | undefined | null
  /** Side in px. 28 in the sidebar header, 20 in its menu, 18 in the sheet. */
  size?: number
  className?: string
}) {
  const mark = teamMark(iconUrl)
  const src = mark.kind === `image` ? mark.src : null
  // Loading or failed = the glyph.
  const [loaded, setLoaded] = useState<string | null>(null)
  useEffect(() => {
    setLoaded(null)
  }, [src])
  return (
    <span
      aria-hidden
      data-slot="team-mark"
      data-mark={src ? `image` : `glyph`}
      className={cn(`relative flex shrink-0 items-center justify-center`, className)}
      style={{ width: size, height: size }}
    >
      {loaded !== src || !src ? <TeamGlyph style={{ width: size, height: size }} /> : null}
      {src && (
        <img
          src={src}
          alt=""
          draggable={false}
          onLoad={() => setLoaded(src)}
          onError={() => setLoaded(null)}
          className={cn(`absolute inset-0 size-full object-cover`, loaded !== src && `opacity-0`)}
          style={{ borderRadius: size * TEAM_IMAGE_MARK.radiusFraction }}
        />
      )}
    </span>
  )
}

/** The Settings › General row (team-image.json): the mark, Upload, Remove. */
export function TeamImageRow({
  iconUrl,
  busy = false,
  onUpload,
  onRemove,
}: {
  iconUrl: string | undefined | null
  busy?: boolean
  onUpload: (file: File) => void
  onRemove: () => void
}) {
  const input = useRef<HTMLInputElement | null>(null)
  return (
    <div data-testid={TEAM_IMAGE_TEST_IDS.row} className="flex items-center gap-3 px-4 py-3">
      <span className="min-w-0 flex-1 text-sm">{TEAM_IMAGE_COPY.row}</span>
      <TeamAvatar iconUrl={iconUrl} size={TEAM_IMAGE_ROW.markSize} />
      <input
        ref={input}
        type="file"
        hidden
        accept={TEAM_IMAGE_UPLOAD.accept.join(`,`)}
        onChange={(event) => {
          const file = event.target.files?.[0]
          event.target.value = ``
          if (file) onUpload(file)
        }}
      />
      <Pill
        mode="action"
        className="shrink-0"
        disabled={busy}
        data-testid={TEAM_IMAGE_TEST_IDS.upload}
        onClick={() => input.current?.click()}
      >
        {TEAM_IMAGE_COPY.upload}
      </Pill>
      {teamMark(iconUrl).kind === `image` && (
        <Button
          variant="ghost"
          size="icon-sm"
          className="size-7 shrink-0 text-muted-foreground"
          disabled={busy}
          aria-label={TEAM_IMAGE_COPY.remove}
          title={TEAM_IMAGE_COPY.remove}
          data-testid={TEAM_IMAGE_TEST_IDS.remove}
          onClick={onRemove}
        >
          <RemoveIcon className="size-4" />
        </Button>
      )}
    </div>
  )
}
