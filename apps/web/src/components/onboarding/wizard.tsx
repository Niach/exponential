import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { FolderKanban, Github, Sparkles, X } from "lucide-react"
import type { BoardIcon } from "@exp/db-schema/domain"
import { trpc } from "@/lib/trpc-client"
import { useCreateBoard } from "@/hooks/use-create-board"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { ColorSwatchGrid } from "@/components/ui/color-swatch-grid"
import { IconSwatchGrid } from "@/components/ui/icon-swatch-grid"
import {
  GithubRepoPicker,
  type PickerRepo,
} from "@/components/github-repo-picker"
import { derivePrefix } from "@/lib/board"

// Onboarding: one form — name/prefix/icon/color and an optional repository.
// A repository is never required, which is what makes onboarding possible on
// instances without a GitHub App. Invited users never reach onboarding (they
// land in the shared team).
export function OnboardingWizard({
  teamId,
  teamSlug,
}: {
  teamId: string
  teamSlug: string
}) {
  const navigate = useNavigate()
  const { createBoard } = useCreateBoard()
  const [name, setName] = useState(``)
  const [prefix, setPrefix] = useState(``)
  const [color, setColor] = useState(`#6366f1`)
  const [icon, setIcon] = useState<BoardIcon>(`code`)
  const [showRepo, setShowRepo] = useState(false)
  const [repo, setRepo] = useState<PickerRepo | null>(null)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // Plan-cap failures render as a softer nudge than hard errors.
  const [limitError, setLimitError] = useState<string | null>(null)

  const handleNameChange = (value: string) => {
    setName(value)
    setPrefix(derivePrefix(value))
  }

  const canCreate = !!name.trim() && !!prefix.trim() && !saving

  const handleCreate = async () => {
    if (!name.trim() || !prefix.trim()) return
    setSaving(true)
    setError(null)
    setLimitError(null)
    const result = await createBoard({
      teamId,
      name,
      prefix,
      color,
      icon,
      repository: repo
        ? {
            fullName: repo.fullName,
            defaultBranch: repo.defaultBranch,
            private: repo.private,
          }
        : undefined,
    })
    if (result.ok) {
      await trpc.onboarding.complete.mutate()
      navigate({ to: `/t/$teamSlug`, params: { teamSlug } })
      return
    }
    if (result.error.kind === `planLimit`) {
      setLimitError(result.error.message)
    } else {
      setError(result.error.message)
    }
    setSaving(false)
  }

  return (
    <div className="flex min-h-screen items-center justify-center p-6">
      <div className="w-full max-w-2xl">
        <Card>
          <CardHeader className="text-center">
            <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-primary/10">
              <FolderKanban className="size-6 text-primary" />
            </div>
            <CardTitle className="text-xl">
              Create your first board
            </CardTitle>
            <CardDescription>
              Boards hold your issues. Connect a GitHub repository to code on
              them — everything can be changed later.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-[1fr_auto]">
              <div className="space-y-2">
                <Label htmlFor="onb-board-name">Board name</Label>
                <Input
                  id="onb-board-name"
                  value={name}
                  onChange={(e) => handleNameChange(e.target.value)}
                  placeholder="e.g. Backend API"
                  autoFocus
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="onb-board-prefix">Prefix</Label>
                <Input
                  id="onb-board-prefix"
                  className="sm:w-28"
                  value={prefix}
                  // Alphanumeric only — the server floor rejects symbol
                  // prefixes (EXP-46).
                  onChange={(e) =>
                    setPrefix(
                      e.target.value.replace(/[^A-Za-z0-9]/g, ``).toUpperCase()
                    )
                  }
                  placeholder="e.g. API"
                  maxLength={10}
                />
              </div>
            </div>

            <div className="space-y-2">
              <Label>Icon</Label>
              <IconSwatchGrid value={icon} onChange={setIcon} color={color} />
            </div>

            <div className="space-y-2">
              <Label>Color</Label>
              <ColorSwatchGrid value={color} onChange={setColor} />
            </div>

            <div className="space-y-2 border-t pt-4">
              <Label>Repository (optional)</Label>
              {repo ? (
                <div className="flex items-center gap-2 rounded-md border px-3 py-2 text-sm">
                  <Github className="h-4 w-4 shrink-0 text-muted-foreground" />
                  <span className="min-w-0 flex-1 truncate">
                    {repo.fullName}
                  </span>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="h-6 px-2 text-xs text-muted-foreground"
                    onClick={() => setRepo(null)}
                  >
                    <X className="mr-1 h-3.5 w-3.5" />
                    Change
                  </Button>
                </div>
              ) : showRepo ? (
                <GithubRepoPicker teamId={teamId} onSelect={setRepo} />
              ) : (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="w-full justify-start text-muted-foreground"
                  onClick={() => setShowRepo(true)}
                >
                  <Github className="mr-2 h-4 w-4" />
                  Connect a GitHub repository
                </Button>
              )}
            </div>

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

            <div className="flex justify-end">
              <Button onClick={() => void handleCreate()} disabled={!canCreate}>
                {saving ? `Creating…` : `Create board`}
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
