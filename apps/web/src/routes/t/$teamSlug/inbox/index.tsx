import { createFileRoute } from "@tanstack/react-router"
import { useSession } from "@/hooks/use-session"
import { InboxView } from "@/components/inbox/inbox-view"

export const Route = createFileRoute(`/t/$teamSlug/inbox/`)({
  component: InboxPage,
})

function InboxPage() {
  const { teamSlug } = Route.useParams()
  const { data: session } = useSession()

  if (!session?.user) return null

  return <InboxView teamSlug={teamSlug} />
}
