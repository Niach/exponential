import { useCallback, useEffect, useMemo, useState } from "react"
import { seedFromSearch, type AgentSearch, type LaunchSeed } from "@/lib/launch-seed"
import { clearPendingShare, loadPendingShare, shareChatSeed } from "@/lib/share/pending-share"

// The home composer's one-shot seed (`/home` and the phones'
// `/t/$teamSlug/agent` alike). A deep link (a native client, a notification,
// an old bookmark) seeds it through the search params, consumed once and
// stripped with a replace navigation (`strip`):
//
//   issues  csv of issue ids (chips; 2+ = a batch)
//   device  the machine to pre-pick
//   text    inserted into the empty draft
//   share   EXP-1635: the pending share (`lib/share/pending-share.ts`) lands
//           in the composer, the agent row on this pick; nothing is sent

export function useHomeSeed(
  search: AgentSearch & { share?: string },
  strip: () => void
): { seed: LaunchSeed | null; onSeedConsumed: () => void } {
  const urlSeed = useMemo(
    () => seedFromSearch(search),
    // Field-wise: the search object's identity is the router's business.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [search.issues, search.device, search.text]
  )
  // Held here after the URL keys are stripped, handed to the composer,
  // cleared once it reports consumption.
  const [seed, setSeed] = useState<LaunchSeed | null>(null)
  const onSeedConsumed = useCallback(() => {
    setSeed((current) => {
      if (current?.pick) void clearPendingShare()
      return null
    })
  }, [])
  useEffect(() => {
    if (!urlSeed) return
    setSeed(urlSeed)
    strip()
  }, [urlSeed, strip])
  // EXP-1635: the Share to page's pick. The share stays pending until the
  // composer took it, so a reload before that still lands it.
  useEffect(() => {
    if (!search.share) return
    const pick = search.share
    let current = true
    void loadPendingShare().then((share) => {
      if (!current) return
      if (share) setSeed(shareChatSeed(share, pick))
      strip()
    })
    return () => {
      current = false
    }
  }, [search.share, strip])
  return { seed, onSeedConsumed }
}
