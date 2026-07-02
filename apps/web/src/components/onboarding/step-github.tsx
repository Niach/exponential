import { useState } from "react"
import { Github, Loader2, Sparkles } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import { invalidateBillingCache } from "@/hooks/use-billing"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  GithubRepoPicker,
  type PickerRepo,
} from "@/components/github-repo-picker"
import type { StepProps } from "./wizard"

// Connect a GitHub repo and link it as the just-created project's primary
// repo, so "Start coding" works from day one. Every state stays skippable —
// the picker's "Track without a repo" and the wizard's global skip both
// move on without a repo.
export function StepGithub({
  workspaceId,
  projectId,
  onNext,
  onSkip,
}: StepProps) {
  const [connecting, setConnecting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // Plan-cap failures (PRECONDITION_FAILED from lib/billing.ts) render as a
  // softer nudge than hard errors like "App not installed on this repo".
  const [limitError, setLimitError] = useState<string | null>(null)

  const handleConnect = async (repo: PickerRepo) => {
    if (connecting) return
    setConnecting(true)
    setError(null)
    setLimitError(null)
    try {
      const { repository } = await trpc.repositories.add.mutate(
        {
          workspaceId,
          fullName: repo.fullName,
          defaultBranch: repo.defaultBranch,
          private: repo.private,
          installationId: repo.installationId,
        },
        // Failures render inline below; the global mutation-error toast
        // would be redundant noise.
        { context: { skipErrorToast: true } }
      )
      invalidateBillingCache()
      if (projectId && repository) {
        await trpc.repositories.linkProject.mutate(
          { projectId, repositoryId: repository.id, isPrimary: true },
          { context: { skipErrorToast: true } }
        )
      }
      onNext()
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      if (isPlanLimitError(err)) {
        setLimitError(message)
      } else {
        setError(message)
      }
    } finally {
      setConnecting(false)
    }
  }

  return (
    <Card>
      <CardHeader className="text-center">
        <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-primary/10">
          <Github className="size-6 text-primary" />
        </div>
        <CardTitle className="text-xl">Connect GitHub</CardTitle>
        <CardDescription>
          Link a repository to your project so issues can be coded on right
          away. You can always do this later in workspace settings.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {error && (
          <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {error}
          </div>
        )}
        {limitError && (
          <div className="flex items-start gap-2 rounded-md border border-primary/40 bg-primary/5 px-3 py-2 text-sm">
            <Sparkles className="mt-0.5 h-3.5 w-3.5 shrink-0 text-primary" />
            <span className="min-w-0 flex-1">{limitError}</span>
          </div>
        )}
        {connecting ? (
          <div className="flex items-center gap-2 rounded-md border px-3 py-6 text-sm text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin" />
            Connecting repository…
          </div>
        ) : (
          <GithubRepoPicker onSelect={handleConnect} onSkip={onSkip} />
        )}
      </CardContent>
    </Card>
  )
}
