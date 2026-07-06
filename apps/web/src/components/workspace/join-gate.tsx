import { useState } from "react"
import { Link } from "@tanstack/react-router"
import { Users } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { trpc } from "@/lib/trpc-client"

// Shown to a signed-in NON-member who navigates to a public workspace.
// Public boards only sync once a user explicitly joins, so instead of an
// empty shell we offer the join. After joining we hard-navigate: the member
// row changes every shape's where clause, and a full reload restarts all
// Electric collections cleanly on the new shape handles.
export function WorkspaceJoinGate(props: {
  workspaceSlug: string
  workspaceName: string
  workspaceId: string
}) {
  const [joining, setJoining] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleJoin = async () => {
    setJoining(true)
    setError(null)
    try {
      await trpc.workspaceMembers.join.mutate({
        workspaceId: props.workspaceId,
      })
      window.location.assign(`/w/${props.workspaceSlug}`)
    } catch (e) {
      setError(e instanceof Error ? e.message : `Failed to join`)
      setJoining(false)
    }
  }

  return (
    <div className="min-h-screen flex items-center justify-center p-4">
      <Card className="w-full max-w-md">
        <CardHeader className="text-center">
          <div className="mx-auto mb-2 flex h-10 w-10 items-center justify-center rounded-full bg-muted">
            <Users className="h-5 w-5 text-muted-foreground" />
          </div>
          <CardTitle>Join {props.workspaceName}</CardTitle>
          <CardDescription>
            This is a public board. Join it to browse issues, follow
            discussions and share feedback. You can leave again anytime from
            the board settings.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col items-center gap-3">
          {error && <p className="text-sm text-destructive">{error}</p>}
          <Button onClick={handleJoin} disabled={joining} className="w-full">
            {joining ? `Joining…` : `Join board`}
          </Button>
          <Button variant="ghost" asChild className="w-full">
            <Link to="/w/$workspaceSlug" params={{ workspaceSlug: `default` }}>
              Back to my workspace
            </Link>
          </Button>
        </CardContent>
      </Card>
    </div>
  )
}
