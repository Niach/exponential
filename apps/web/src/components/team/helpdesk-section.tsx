import { useState } from "react"
import { Link } from "@tanstack/react-router"
import { TRPCClientError } from "@trpc/client"
import { trpc } from "@/lib/trpc-client"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import { conceptIcon } from "@/lib/icons.generated"
import { Pill } from "@/components/ui/pill"
import {
  GlassGroup,
  GlassSectionHeader,
  GlassToggleRow,
} from "@/components/ui/glass-rows"
import type { Team } from "@/db/schema"

const HelpdeskIcon = conceptIcon(`settings-helpdesk`)

// EXP-771: the team-level helpdesk switch is its own settings page. It used
// to sit under the widget settings because support tickets arrive through the
// widget; the desktop IDE has the same two panes.
export function TeamHelpdeskSection({ team }: { team: Team }) {
  const teamId = team.id

  // Team-level helpdesk switch (EXP-180 — replaced the per-board flag).
  const [helpdeskBusy, setHelpdeskBusy] = useState(false)
  const [helpdeskError, setHelpdeskError] = useState<string | null>(null)

  const toggleHelpdesk = async (enabled: boolean) => {
    setHelpdeskBusy(true)
    setHelpdeskError(null)
    try {
      await trpc.teams.update.mutate({
        teamId,
        helpdeskEnabled: enabled,
      })
    } catch (err) {
      if (isPlanLimitError(err)) {
        setHelpdeskError(`The helpdesk is available on the Team plan.`)
      } else if (
        err instanceof TRPCClientError &&
        err.data?.code === `PRECONDITION_FAILED`
      ) {
        // A non-plan-limit precondition failure is an actionable SETUP error —
        // REV2-10(c)'s transport gate refuses `helpdeskEnabled: true` with a
        // message naming AWS_SES_REGION / SMTP_HOST. This toggle is the only
        // place that gate is ever hit, so the server's own wording must reach
        // the owner instead of the generic fallback.
        setHelpdeskError(err.message)
      } else {
        setHelpdeskError(`Could not update the helpdesk setting.`)
      }
    } finally {
      setHelpdeskBusy(false)
    }
  }

  return (
    <div className="space-y-6">
      {/* Anchor target for the "Getting started" helpdesk card's link. */}
      <div id="helpdesk" className="scroll-mt-6">
        <GlassSectionHeader
          leading={<HelpdeskIcon className="size-3.5 text-foreground/50" />}
          label="Helpdesk"
        />
        <GlassGroup>
          <GlassToggleRow
            id="team-helpdesk-enabled"
            label="Enable the helpdesk"
            description="Give this team a shared support inbox. Support tickets from the widget land there."
            checked={team.helpdeskEnabled}
            disabled={helpdeskBusy}
            onCheckedChange={(next) => void toggleHelpdesk(next)}
          />
        </GlassGroup>
        {(helpdeskError || team.helpdeskEnabled) && (
          <div className="space-y-2 pt-2">
            {helpdeskError && (
              <p className="text-xs text-destructive">{helpdeskError}</p>
            )}
            {team.helpdeskEnabled && (
              <Pill mode="action" asChild className="w-fit">
                <Link
                  to="/t/$teamSlug/support"
                  params={{ teamSlug: team.slug }}
                >
                  Open Support inbox
                </Link>
              </Pill>
            )}
          </div>
        )}
      </div>
    </div>
  )
}
