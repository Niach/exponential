import type { ReactNode } from "react"
import { useNavigate } from "@tanstack/react-router"
import { MobileDetailHeader, Separator, useIsMobile } from "@exp/ui"
import { PushedTitle } from "@/components/team/mobile-page-header"

// The creation pages' heading (New › Board / Agent / App): md+ the h1 over its
// hairline, a phone (#EXP-2109) the detail header back to the team's page.
export function NewPageHeading({ teamSlug, children }: { teamSlug: string; children: ReactNode }) {
  const navigate = useNavigate()
  if (useIsMobile()) {
    return (
      <div className="-mx-4 -mt-4 pt-[env(safe-area-inset-top)]">
        <MobileDetailHeader
          title={<PushedTitle>{children}</PushedTitle>}
          onBack={() => void navigate({ to: `/t/$teamSlug/team`, params: { teamSlug } })}
        />
      </div>
    )
  }
  return (
    <>
      <h1 className="text-2xl font-bold">{children}</h1>
      <Separator />
    </>
  )
}
