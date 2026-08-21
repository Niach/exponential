import { createFileRoute, Link, redirect } from "@tanstack/react-router"
import { conceptIcon } from "@/lib/icons.generated"
import { TeamActionsPanel } from "@/components/team-actions-panel"
import { useTeamBySlug } from "@/hooks/use-team-data"

const BackIcon = conceptIcon(`ui-back`)

// EXP-574: the mobile Actions page — native-app parity (Agents → top-right
// "Actions" → this pushed detail view). It carries its own back header, so the
// mobile chrome (topbar + tab bar) hides here like on the other detail routes.
// Desktop viewports normally see the same panel inline on the Agents page;
// landing here directly (a shared link, a resize) still renders fine.
export const Route = createFileRoute(`/t/$teamSlug/agents/actions`)({
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: ActionsPage,
})

function ActionsPage() {
  const { teamSlug } = Route.useParams()
  const team = useTeamBySlug(teamSlug)

  return (
    <div className="flex h-full flex-col">
      <header className="sticky top-0 z-30 flex h-12 shrink-0 items-center gap-1 border-b border-border/60 px-2 glass-chrome-top md:hidden">
        <Link
          to="/t/$teamSlug/agents"
          params={{ teamSlug }}
          aria-label="Back"
          className="flex size-9 items-center justify-center rounded-full text-muted-foreground hover:text-foreground"
        >
          <BackIcon className="size-5" />
        </Link>
        <span className="truncate text-sm font-medium">Actions</span>
      </header>
      <div className="flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-3xl px-4 py-4 md:max-w-5xl">
          {team ? (
            <TeamActionsPanel team={team} />
          ) : (
            <div className="p-6 text-sm text-muted-foreground">Loading…</div>
          )}
        </div>
      </div>
    </div>
  )
}
