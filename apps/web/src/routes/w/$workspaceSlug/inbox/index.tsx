import { createFileRoute } from "@tanstack/react-router"
import { useSession } from "@/hooks/use-session"
import { InboxView } from "@/components/inbox/inbox-view"

export const Route = createFileRoute(`/w/$workspaceSlug/inbox/`)({
  component: InboxPage,
})

function InboxPage() {
  const { workspaceSlug } = Route.useParams()
  const { data: session } = useSession()

  if (!session?.user) return null

  return <InboxView workspaceSlug={workspaceSlug} />
}
