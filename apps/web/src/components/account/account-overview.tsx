import { useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { getInitials } from "@/lib/utils"
import { useSession } from "@/hooks/use-session"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

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
  // Name-less accounts (Apple sign-in stores an empty name) fall back to the
  // email for initials instead of a bare "?".
  const userLabel = session?.user?.name || session?.user?.email
  const userInitials = userLabel ? getInitials(userLabel) : `?`
  // Never captured (pre-EXP-369 account that hasn't loaded the app since) →
  // the server reads UTC. EXP-452: the picker used to RENDER that fallback as
  // a selected `UTC`, which is indistinguishable from having deliberately
  // chosen UTC — so an account silently scheduling its digest in the wrong
  // clock looked correctly configured. Unset is now its own visible state.
  const [timezone, setTimezone] = useState<string | null>(initialTimezone)

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
        <Avatar className="h-12 w-12">
          {session?.user?.image && <AvatarImage src={session.user.image} />}
          <AvatarFallback>{userInitials}</AvatarFallback>
        </Avatar>
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
      <div className="flex items-center gap-3">
        <div className="flex-1">
          <Label htmlFor="timezone">Timezone</Label>
          <p className="text-xs text-muted-foreground">
            {timezone
              ? `Used to schedule your daily digest email.`
              : `Not set — your daily digest is scheduled in UTC.`}
          </p>
        </div>
        <Select value={timezone ?? ``} onValueChange={handleTimezone}>
          <SelectTrigger id="timezone" className="w-60">
            <SelectValue placeholder="Not set (UTC)" />
          </SelectTrigger>
          <SelectContent>
            {timezoneOptions(timezone).map((zone) => (
              <SelectItem key={zone} value={zone}>
                {zone}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
    </div>
  )
}
