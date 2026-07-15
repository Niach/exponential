import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { ArrowLeft, FolderKanban, Github, Globe, Sparkles, X } from "lucide-react"
import type { ProjectIcon } from "@exp/db-schema/domain"
import { PROJECT_TEMPLATES, type ProjectTemplate } from "@/lib/project-types"
import { trpc } from "@/lib/trpc-client"
import { useCreateProject } from "@/hooks/use-create-project"
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
import { Switch } from "@/components/ui/switch"
import { ColorSwatchGrid } from "@/components/ui/color-swatch-grid"
import { IconSwatchGrid } from "@/components/ui/icon-swatch-grid"
import {
  GithubRepoPicker,
  type PickerRepo,
} from "@/components/github-repo-picker"
import { derivePrefix } from "@/lib/project"

// Onboarding: pick a creation template first (Dev / Tasks / Feedback
// quickstart — templates only pre-set the public toggle, icon and whether the
// repo picker leads), then one form: name/prefix/icon/color, optional
// repository, public-board switch. A repository is never required, which is
// what makes onboarding possible on instances without a GitHub App. Invited
// users never reach onboarding (they land in the shared workspace).
export function OnboardingWizard({
  workspaceId,
  workspaceSlug,
}: {
  workspaceId: string
  workspaceSlug: string
}) {
  const navigate = useNavigate()
  const { createProject } = useCreateProject()
  const [template, setTemplate] = useState<ProjectTemplate | null>(null)
  const [name, setName] = useState(``)
  const [prefix, setPrefix] = useState(``)
  const [color, setColor] = useState(`#6366f1`)
  const [icon, setIcon] = useState<ProjectIcon>(`code`)
  const [isPublic, setIsPublic] = useState(false)
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

  const applyTemplate = (next: ProjectTemplate) => {
    setTemplate(next)
    setIcon(next.defaults.icon)
    setIsPublic(next.defaults.isPublic)
    setShowRepo(next.defaults.suggestsRepo)
  }

  const canCreate = !!name.trim() && !!prefix.trim() && !saving

  const handleCreate = async () => {
    if (!template || !name.trim() || !prefix.trim()) return
    setSaving(true)
    setError(null)
    setLimitError(null)
    const result = await createProject({
      workspaceId,
      name,
      prefix,
      color,
      icon,
      isPublic,
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
      navigate({ to: `/w/$workspaceSlug`, params: { workspaceSlug } })
      return
    }
    if (result.error.kind === `planLimit`) {
      setLimitError(result.error.message)
    } else {
      setError(result.error.message)
    }
    setSaving(false)
  }

  if (template === null) {
    return (
      <div className="flex min-h-screen items-center justify-center p-6">
        <div className="w-full max-w-2xl">
          <Card>
            <CardHeader className="text-center">
              <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-primary/10">
                <FolderKanban className="size-6 text-primary" />
              </div>
              <CardTitle className="text-xl">
                What are you building?
              </CardTitle>
              <CardDescription>
                Pick a starting point for your first project — everything can
                be changed later.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              {PROJECT_TEMPLATES.map((option) => (
                <button
                  key={option.key}
                  type="button"
                  onClick={() => applyTemplate(option)}
                  className="flex w-full items-start gap-4 rounded-lg border border-border p-4 text-left transition-colors hover:border-primary/60 hover:bg-accent/40"
                >
                  <option.icon className="mt-1 h-6 w-6 shrink-0 text-primary" />
                  <span className="min-w-0">
                    <span className="flex items-center gap-1.5 font-medium">
                      {option.label}
                      {option.defaults.isPublic && (
                        <Globe className="h-4 w-4 text-muted-foreground" />
                      )}
                    </span>
                    <span className="block text-sm text-muted-foreground">
                      {option.description}
                    </span>
                  </span>
                </button>
              ))}
            </CardContent>
          </Card>
        </div>
      </div>
    )
  }

  return (
    <div className="flex min-h-screen items-center justify-center p-6">
      <div className="w-full max-w-2xl">
        <Card>
          <CardHeader className="text-center">
            <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-primary/10">
              <template.icon className="size-6 text-primary" />
            </div>
            <CardTitle className="text-xl">
              Create your {template.label.toLowerCase()}
            </CardTitle>
            <CardDescription>{template.description}</CardDescription>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="mx-auto mt-1 h-7 px-2 text-xs text-muted-foreground"
              onClick={() => setTemplate(null)}
            >
              <ArrowLeft className="mr-1 h-3.5 w-3.5" />
              Choose a different template
            </Button>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-[1fr_auto]">
              <div className="space-y-2">
                <Label htmlFor="onb-project-name">Project name</Label>
                <Input
                  id="onb-project-name"
                  value={name}
                  onChange={(e) => handleNameChange(e.target.value)}
                  placeholder="e.g. Backend API"
                  autoFocus
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="onb-project-prefix">Prefix</Label>
                <Input
                  id="onb-project-prefix"
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
                <GithubRepoPicker workspaceId={workspaceId} onSelect={setRepo} />
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

            <div className="flex items-center justify-between rounded-md border border-border px-3 py-2">
              <div className="min-w-0">
                <Label
                  htmlFor="onb-project-public"
                  className="flex items-center gap-1.5 text-sm"
                >
                  <Globe className="h-3.5 w-3.5 text-muted-foreground" />
                  Public board
                </Label>
                <p className="text-xs text-muted-foreground">
                  Anyone with the link can read it.
                </p>
              </div>
              <Switch
                id="onb-project-public"
                checked={isPublic}
                onCheckedChange={setIsPublic}
              />
            </div>

            {isPublic && (
              <p className="rounded-md border border-border bg-accent/30 px-3 py-2 text-xs text-muted-foreground">
                Public boards are readable by anyone: issues, comments and
                @mentions in them are visible to anyone with the link. The
                workspace name is shown on the board.
              </p>
            )}

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
                {saving ? `Creating…` : `Create project`}
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
