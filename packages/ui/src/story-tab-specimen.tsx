import { STORY_TAB_SPECIMEN } from "@exp/domain-contract/stories"
import { useSpecimenAttachmentSrc } from "./specimen-samples"
import { StoryTab, type StoryTabSize } from "./story-tab"

// STORIES TAB: THE styleguide specimen (stories.json `tab.specimen` =
// tabCases[1]'s ring), drawn by the styleguide entry `story-tab` and the
// app's DEV-ONLY `?dev=story-tab` capture: the tab body wide (md+ / IDE)
// and phone, no PR block. The specimen's pictures are the committed samples
// (EXP-1654, specimen-samples.ts), never fetched.

export function StoryTabSpecimen({ width = 640, size = `wide` }: { width?: number; size?: StoryTabSize }) {
  const attachmentSrc = useSpecimenAttachmentSrc()
  return (
    <div
      data-testid="story-tab-specimen"
      // #EXP-1553: phones draw the tab on the bare page like iOS/Android
      // (no card frame), so the phone thumbnail row has the natives' room.
      className="rounded-3xl border border-glass-stroke-card bg-background p-5 max-md:rounded-none max-md:border-0 max-md:p-0"
      style={{ width, maxWidth: `100%` }}
    >
      <StoryTab ring={STORY_TAB_SPECIMEN} size={size} attachmentSrc={attachmentSrc} onOpen={() => {}} />
    </div>
  )
}
