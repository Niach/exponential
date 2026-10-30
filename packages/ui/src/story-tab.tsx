import type { ReactNode } from "react"
import type { CardSurface } from "@exp/domain-contract/plan-card"
import { STORIES_COPY, STORY_TAB, type Story, type StoryPicture, type StoryRing } from "@exp/domain-contract/stories"
import { cn } from "./cn"
import { EmptyState } from "./empty-state"
import { conceptIcon } from "./icons.generated"
import { SurfaceCard } from "./surface-card"

// STORIES TAB ×web (fixtures/stories.json STORIES TAB; desktop crates/ui
// story_tab, iOS ExpUI StoryTab, Android ui/stories StoryTab): the session
// screen's Stories tab body. The model is the contract's `storyTab()` ring;
// these only paint it.
//
//   StoryTab       the PR block slot (`lead`), then the stories INLINE in
//                  fold order `tile.listGap` apart (#EXP-1462: the Guide's
//                  way, never a list of title rows), else THE EmptyState
//   StoryTile      one story's section: line 1 = the title (one line; none
//                  = the kind word) with `meta` muted at the end (a waiting
//                  one leads with the plan/question tint dot and tints its
//                  meta), the description (muted, wrapping), then its body
//                  in place: ONE row of pictures (a muted caption-or-label
//                  under each), else THE surface card, else the plan text

const StoriesIcon = conceptIcon(`thread-stories`)
const TILE = STORY_TAB.tile

/** The waiting tint (ask-dock: plan primary, question yellow). */
function waitingTint(story: Pick<Story, `kind`>): { dot: string; text: string } {
  return story.kind === `question`
    ? { dot: `bg-yellow-400`, text: `text-yellow-400` }
    : { dot: `bg-primary`, text: `text-primary` }
}

/** `wide` = web md+ (and the IDE), `phone` = web-mobile (and the natives). */
export type StoryTabSize = `wide` | `phone`

/** A thumbnail's width (px): by the picture's aspect at `tile.thumbHeight`,
 *  capped at `tile.thumbMaxWidth` (both per size); an unknown size = 4:3. */
export function storyThumbWidth(picture: Pick<StoryPicture, `width` | `height`>, size: StoryTabSize): number {
  const aspect = picture.width && picture.height ? picture.width / picture.height : 4 / 3
  return Math.min(TILE.thumbMaxWidth[size], Math.round(TILE.thumbHeight[size] * aspect))
}

/** How a host draws a story's body in place (absent = the line alone). */
export interface StoryBodyRenderers {
  /** The story's surface (its raw card's), null = none. */
  surfaceFor?: (story: Story) => CardSurface | null
  /** A surface press's reply text (THE chat's `surfaceReply`). */
  onSurfaceReply?: (story: Story, text: string) => void
  /** The plan markdown (absent = plain text). */
  renderText?: (text: string) => ReactNode
}

export function StoryTile({
  story,
  size,
  attachmentSrc,
  onOpen,
  surfaceFor,
  onSurfaceReply,
  renderText,
}: {
  story: Story
  size: StoryTabSize
  attachmentSrc: (attachmentId: string) => string
  /** A tap on the line (picture 0) or a picture (its index). */
  onOpen?: (story: Story, picture: number) => void
} & StoryBodyRenderers) {
  const tint = story.waiting ? waitingTint(story) : null
  // The frame rule: pictures win, else the surface, else the plan text.
  const surface = story.pictures.length === 0 && story.surface ? (surfaceFor?.(story) ?? null) : null
  const text = story.pictures.length === 0 && !surface ? story.text : null
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
        className="flex min-w-0 flex-col rounded-md text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
        style={{ gap: TILE.textGap }}
        onClick={() => onOpen?.(story, 0)}
      >
        <span className="flex min-w-0 items-center gap-2">
          {tint && <span aria-hidden className={cn(`size-1.5 shrink-0 rounded-full`, tint.dot)} />}
          <span className="min-w-0 flex-1 truncate text-sm font-medium text-foreground">
            {story.title ?? STORIES_COPY[story.kind]}
          </span>
          <span className={cn(`shrink-0 text-xs`, tint ? tint.text : `text-muted-foreground`)}>{story.meta}</span>
        </span>
        {story.description && (
          <span data-slot="story-tile-description" className="text-sm text-muted-foreground">
            {story.description}
          </span>
        )}
      </button>
      {story.pictures.length > 0 && (
        <div
          data-slot="story-tile-pictures"
          className="flex min-w-0 overflow-x-auto overscroll-x-contain [scrollbar-width:none]"
          style={{ gap: TILE.gap }}
        >
          {story.pictures.map((picture, index) => {
            const width = storyThumbWidth(picture, size)
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
                  draggable={false}
                  className="block border border-glass-stroke bg-muted object-cover object-top"
                  style={{ width, height: TILE.thumbHeight[size], borderRadius: TILE.radius }}
                />
                <span className="truncate text-xs text-muted-foreground">{picture.caption ?? picture.label}</span>
              </button>
            )
          })}
        </div>
      )}
      {surface && (
        <div data-slot="story-tile-surface" className="min-w-0">
          <SurfaceCard
            surface={surface}
            testKey={`tab-${story.id}`}
            onReply={onSurfaceReply ? (reply) => onSurfaceReply(story, reply) : undefined}
          />
        </div>
      )}
      {text && (
        <div data-slot="story-tile-text" className="min-w-0 text-sm text-foreground/90">
          {renderText ? renderText(text) : <p className="whitespace-pre-wrap">{text}</p>}
        </div>
      )}
    </div>
  )
}

/** THE Stories tab body (`storyTab()`'s ring; null = the empty state). */
export function StoryTab({
  ring,
  size,
  lead,
  attachmentSrc = (id) => `/api/attachments/${id}`,
  onOpen,
  surfaceFor,
  onSurfaceReply,
  renderText,
  className,
}: {
  ring: Pick<StoryRing, `stories`> | null
  size: StoryTabSize
  /** The PR block (the Stack band + StackRail, the Changes row). */
  lead?: ReactNode
  attachmentSrc?: (attachmentId: string) => string
  onOpen?: (story: Story, picture: number) => void
  className?: string
} & StoryBodyRenderers) {
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
            <StoryTile
              key={story.id}
              story={story}
              size={size}
              attachmentSrc={attachmentSrc}
              onOpen={onOpen}
              surfaceFor={surfaceFor}
              onSurfaceReply={onSurfaceReply}
              renderText={renderText}
            />
          ))}
        </div>
      ) : (
        <EmptyState icon={StoriesIcon} title={STORY_TAB.copy.empty} size="page" />
      )}
    </div>
  )
}
