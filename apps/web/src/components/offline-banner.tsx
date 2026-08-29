import { Button } from "@/components/ui/button"
import { useConnectivity } from "@/hooks/use-connectivity"
import { conceptIcon } from "@/lib/icons.generated"

const OfflineIcon = conceptIcon(`ui-offline`)

/**
 * EXP-533: parity with the iOS and desktop banner. The boards keep rendering
 * from the synced cache while the server is unreachable, which is the right
 * behaviour and also indistinguishable from "nothing is happening" — so say
 * so, and offer the one action that helps.
 *
 * Amber rather than destructive: nothing is broken and nothing was lost, the
 * data on screen is just not live. (There is no `warning` token in
 * `styles.css`, hence the literal amber utilities.)
 */
export function OfflineBanner() {
  const { health, retry, retrying } = useConnectivity()
  if (health !== `offline`) return null
  return (
    <div
      role="status"
      className="flex items-center gap-2 border-b border-amber-500/30 bg-amber-500/10 px-4 py-1.5 text-xs text-amber-400"
    >
      <OfflineIcon className="size-3.5 shrink-0" />
      {/* Byte-identical to the iOS, Android and desktop banners — kept as a
          string literal so a cross-client grep can lock the copy. */}
      <span className="min-w-0 flex-1">
        {`Can't reach the server, showing cached data`}
      </span>
      <Button
        variant="ghost"
        size="sm"
        className="h-6 px-2 text-amber-400 hover:bg-amber-500/15 hover:text-amber-300"
        disabled={retrying}
        onClick={retry}
      >
        {retrying ? `Retrying…` : `Retry`}
      </Button>
    </div>
  )
}
