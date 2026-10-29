import type { ReactNode } from "react"
import { STORIES_COPY, STORY_TAB, type Story, type StoryPicture, type StoryRing } from "@exp/domain-contract/stories"
import { cn } from "./cn"
import { EmptyState } from "./empty-state"
import { conceptIcon } from "./icons.generated"

// STORIES TAB ×web (fixtures/stories.json STORIES TAB; desktop crates/ui
// story_tab, iOS ExpUI StoryTab, Android ui/stories StoryTab): the session
// screen's Stories tab body. The model is the contract's `storyTab()` ring;
// these only paint it.
//
//   StoryTab       the PR block slot (`lead`), then the tiles in fold
//                  order `tile.listGap` apart, else THE EmptyState
//   StoryTile      line 1 = the title (one line; none = the kind word) with
//                  `meta` muted at the end (a waiting tile leads with the
//                  plan/question tint dot and tints its meta), then ONE row
//                  of picture thumbnails, a muted caption-or-label under each

const StoriesIcon = conceptIcon(`thread-stories`)
const TILE = STORY_TAB.tile

/** The waiting tint (ask-dock: plan primary, question yellow). */
function waitingTint(story: Pick<Story, `kind`>): { dot: string; text: string } {
  return story.kind === `question`
    ? { dot: `bg-yellow-400`, text: `text-yellow-400` }
    : { dot: `bg-primary`, text: `text-primary` }
}

/** A thumbnail's width (px): by the picture's aspect at `tile.thumbHeight`,
 *  capped at `tile.thumbMaxWidth`; an unknown size = 4:3. */
export function storyThumbWidth(picture: Pick<StoryPicture, `width` | `height`>): number {
  const aspect = picture.width && picture.height ? picture.width / picture.height : 4 / 3
  return Math.min(TILE.thumbMaxWidth, Math.round(TILE.thumbHeight * aspect))
}

export function StoryTile({
  story,
  attachmentSrc,
  onOpen,
}: {
  story: Story
  attachmentSrc: (attachmentId: string) => string
  /** A tap on the tile (picture 0) or a thumbnail (its index). */
  onOpen?: (story: Story, picture: number) => void
}) {
  const tint = story.waiting ? waitingTint(story) : null
  return (
    <div
      data-testid={STORY_TAB.testIds.tile}
      data-story={story.id}
      data-waiting={story.waiting || undefined}
      className="flex min-w-0 flex-col"
      style={{ gap: TILE.gap }}
    >
      <button
        type="button"
        className="flex min-w-0 items-center gap-2 rounded-md text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
        onClick={() => onOpen?.(story, 0)}
      >
        {tint && <span aria-hidden className={cn(`size-1.5 shrink-0 rounded-full`, tint.dot)} />}
        <span className="min-w-0 flex-1 truncate text-sm font-medium text-foreground">
          {story.title ?? STORIES_COPY[story.kind]}
        </span>
        <span className={cn(`shrink-0 text-xs`, tint ? tint.text : `text-muted-foreground`)}>{story.meta}</span>
      </button>
      {story.pictures.length > 0 && (
        <div
          data-slot="story-tile-pictures"
          className="flex min-w-0 overflow-x-auto overscroll-x-contain [scrollbar-width:none]"
          style={{ gap: TILE.gap }}
        >
          {story.pictures.map((picture, index) => {
            const width = storyThumbWidth(picture)
            return (
              <button
                key={`${picture.attachmentId}-${index}`}
                type="button"
                data-slot="story-tile-picture"
                className="flex shrink-0 flex-col gap-1.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
                style={{ width }}
                onClick={() => onOpen?.(story, index)}
              >
                <img
                  src={attachmentSrc(picture.attachmentId)}
                  alt={picture.caption ?? picture.label}
                  loading="lazy"
                  draggable={false}
                  className="block border border-glass-stroke bg-muted object-cover object-top"
                  style={{ width, height: TILE.thumbHeight, borderRadius: TILE.radius }}
                />
                <span className="truncate text-xs text-muted-foreground">{picture.caption ?? picture.label}</span>
              </button>
            )
          })}
        </div>
      )}
    </div>
  )
}

/** THE Stories tab body (`storyTab()`'s ring; null = the empty state). */
export function StoryTab({
  ring,
  lead,
  attachmentSrc = (id) => `/api/attachments/${id}`,
  onOpen,
  className,
}: {
  ring: Pick<StoryRing, `stories`> | null
  /** The PR block (the Stack band + StackRail, the Changes row). */
  lead?: ReactNode
  attachmentSrc?: (attachmentId: string) => string
  onOpen?: (story: Story, picture: number) => void
  className?: string
}) {
  const stories = ring?.stories ?? []
  return (
    <div
      data-testid={STORY_TAB.testIds.list}
      className={cn(`flex min-w-0 flex-col`, className)}
      style={{ gap: TILE.leadGap }}
    >
      {lead}
      {stories.length > 0 ? (
        <div className="flex min-w-0 flex-col" style={{ gap: TILE.listGap }}>
          {stories.map((story) => (
            <StoryTile key={story.id} story={story} attachmentSrc={attachmentSrc} onOpen={onOpen} />
          ))}
        </div>
      ) : (
        <EmptyState icon={StoriesIcon} title={STORY_TAB.copy.empty} size="page" />
      )}
    </div>
  )
}
