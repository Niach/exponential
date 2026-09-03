import { useGettingStartedProgressContext } from "@/hooks/use-getting-started-progress"
import {
  GettingStartedCards,
  type GettingStartedCardsProps,
} from "@/components/getting-started/getting-started-cards"
import { GlassSectionHeader } from "@/components/ui/glass-rows"

// The "Getting started" block under the board's "No issues yet" empty state
// (EXP-88). EXP-548: no dismissal — the block (like the sidebar entry and the
// IDE's rail entry) simply disappears once every entry is done, and stays
// hidden while the signals load so a done user never sees it flash.
export function GettingStartedSection({
  team,
  teamSlug,
}: Omit<GettingStartedCardsProps, `layout`>) {
  const { loading, complete } = useGettingStartedProgressContext()
  if (loading || complete) return null

  return (
    <div className="mx-auto w-full max-w-4xl px-6 pb-12">
      <GlassSectionHeader label="Getting started" />
      <GettingStartedCards
        team={team}
        teamSlug={teamSlug}
        layout="grid"
      />
    </div>
  )
}
