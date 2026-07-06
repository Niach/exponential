import { useState } from "react"
import { UserMinus } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { trpc } from "@/lib/trpc-client"

// Public boards don't expose member management (identities are anonymized and
// joins are self-service) — the only membership action is leaving. Leaving
// changes every shape's where clause, so we hard-navigate afterwards to
// restart the Electric collections.
export function LeaveBoardSection({ memberId }: { memberId: string }) {
  const [leaving, setLeaving] = useState(false)

  const handleLeave = async () => {
    setLeaving(true)
    try {
      await trpc.workspaceMembers.remove.mutate({ memberId })
      window.location.assign(`/w/default`)
    } catch {
      setLeaving(false)
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Membership</CardTitle>
        <CardDescription>
          This is a public board — members are anonymous to each other, and you
          can leave at any time. You can rejoin later whenever you want to
          share feedback again.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <Button variant="outline" onClick={handleLeave} disabled={leaving}>
          <UserMinus className="mr-2 h-4 w-4" />
          {leaving ? `Leaving…` : `Leave this board`}
        </Button>
      </CardContent>
    </Card>
  )
}
