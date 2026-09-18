import { useState } from "react"
import { trpc } from "@/lib/trpc-client"
import {
  Combobox,
  GlassGroup,
  UserAvatar,
} from "@exp/ui"
import { useSession } from "@/hooks/use-session"

// `Intl.supportedValuesOf` is ES2023 — the app targets ES2022, so it is read
// through a widened type and treated as optional at runtime too (older
// engines simply get the short fallback list).
const intlWithSupportedValues = Intl as typeof Intl & {
  supportedValuesOf?: (key: string) => string[]
}

function timezoneOptions(current: string | null): string[] {
  let zones: string[] = []
  try {
    zones = intlWithSupportedValues.supportedValuesOf?.(`timeZone`) ?? []
  } catch {
    zones = []
  }
  if (zones.length === 0) {
    const local = Intl.DateTimeFormat().resolvedOptions().timeZone
    zones = [...new Set([`UTC`, local].filter(Boolean))]
  }
  // A stored zone the runtime doesn't list (older/newer tzdata, or a value set
  // by another client) must still render as the selected option.
  if (current && !zones.includes(current)) zones = [current, ...zones]
  return zones
}

// The identity block + timezone picker, shared by the settings Account
// section. Split out of the old /account/notifications page (EXP-238).
export function AccountOverview({
  initialTimezone,
}: {
  initialTimezone: string | null
}) {
  const { data: session } = useSession()
  // Never captured (pre-EXP-369 account that hasn't loaded the app since) →
  // the server reads UTC, so that is what the picker shows.
  const [timezone, setTimezone] = useState(initialTimezone ?? `UTC`)

  const handleTimezone = (next: string) => {
    setTimezone(next)
    void trpc.users.setTimezone
      .mutate({ timezone: next })
      .catch((err) => console.error(`[prefs] update failed:`, err))
  }

  return (
    <div className="space-y-6">
      {/* EXP-311: the chrome shows only the first name — the full identity
          (name + email) lives here. */}
      <div className="flex items-center gap-3">
        <UserAvatar size={48} user={session?.user} />
        <div className="min-w-0">
          <div className="truncate font-medium">
            {session?.user?.name || session?.user?.email}
          </div>
          {session?.user?.name && (
            <div className="truncate text-sm text-muted-foreground">
              {session.user.email}
            </div>
          )}
        </div>
      </div>

      {/* EXP-369: the account's clock — the daily digest's send hour is read
          in it. Captured from the browser on first load; explicit here. */}
      <GlassGroup>
        <div className="flex flex-col">
          {/* EXP-958: the ONE settings row that keeps its search field — the
              tz database is hundreds of zones, and the Select this replaced
              had type-ahead of its own. */}
          <Combobox
            triggerVariant="row"
            mobileTitle="Timezone"
            value={timezone}
            onChange={(zone) => {
              if (zone !== null) handleTimezone(zone)
            }}
            options={timezoneOptions(timezone).map((zone) => ({
              value: zone,
              label: zone,
            }))}
            placeholder="Search zones…"
            emptyText="No matching timezone."
            width="lg"
          />
          <p className="px-4 pb-3 text-xs text-foreground/50">
            Used to schedule your daily digest email.
          </p>
        </div>
      </GlassGroup>
    </div>
  )
}
