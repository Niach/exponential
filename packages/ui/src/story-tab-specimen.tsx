import { STORY_TAB_SPECIMEN } from "@exp/domain-contract/stories"
import { StoryTab } from "./story-tab"

// STORIES TAB: THE styleguide specimen (stories.json `tab.specimen` =
// tabCases[1]'s ring), drawn by the styleguide entry `story-tab` and the
// app's DEV-ONLY `?dev=story-tab` capture: the tab body wide (md+ / IDE)
// and phone, no PR block. The specimen's pictures are placeholders: the
// thumbnail's muted fill alone (natives draw the same muted rounded rect).

/** A transparent pixel: the thumbnail's `bg-muted` shows through. */
const PLACEHOLDER = `data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7`
const placeholderSrc = () => PLACEHOLDER

export function StoryTabSpecimen({ width = 640 }: { width?: number }) {
  return (
    <div
      data-testid="story-tab-specimen"
      className="rounded-3xl border border-glass-stroke-card bg-background p-5"
      style={{ width, maxWidth: `100%` }}
    >
      <StoryTab ring={STORY_TAB_SPECIMEN} attachmentSrc={placeholderSrc} onOpen={() => {}} />
    </div>
  )
}
