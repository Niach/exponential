import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import {
  ArrowLeft,
  FolderKanban,
  Github,
  Link as LinkIcon,
  Plus,
  Sparkles,
  Users,
  X,
} from "lucide-react"
import type { BoardIcon } from "@exp/db-schema/domain"
import { trpc } from "@/lib/trpc-client"
import { isPlanLimitError } from "@/lib/plan-limit-error"
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

// Onboarding (EXP-188): signups get no team anymore, so the wizard is a
// step machine — choice → create-team | join → board. `initialTeam` (resolved
// by the route via teams.getDefault) skips straight to the board step: the
// resumed-onboarding case (team exists but onboarding never completed). The
// join path leaves the wizard entirely — accepting an invite marks onboarding
// complete server-side, so invited users never see the board step.
type WizardStep =
  | { kind: `choice` }
  | { kind: `create-team` }
  | { kind: `join` }
  | { kind: `board`; team: { id: string; slug: string } }

// Accepts a full invite link (…/invite/<token>) or a bare 64-hex token —
// the form teamInvites.create mints (randomBytes(32).toString("hex")).
export function extractInviteToken(input: string): string | null {
  const trimmed = input.trim()
  if (!trimmed) return null
  const linkMatch = /\/invite\/([A-Za-z0-9]+)/.exec(trimmed)
  if (linkMatch) return linkMatch[1]
  if (/^[0-9a-fA-F]{64}$/.test(trimmed)) return trimmed
  return null
}

export function OnboardingWizard({
  initialTeam,
}: {
  initialTeam: { id: string; slug: string } | null
}) {
  const [step, setStep] = useState<WizardStep>(
    initialTeam
      ? { kind: `board`, team: initialTeam }
      : { kind: `choice` }
  )

  return (
    <div className="flex min-h-screen items-center justify-center p-6">
      <div className="w-full max-w-2xl">
        {step.kind === `choice` && (
          <ChoiceStep
            onCreate={() => setStep({ kind: `create-team` })}
            onJoin={() => setStep({ kind: `join` })}
          />
        )}
        {step.kind === `create-team` && (
          <CreateTeamStep
            onBack={() => setStep({ kind: `choice` })}
            onCreated={(team) => setStep({ kind: `board`, team })}
          />
        )}
        {step.kind === `join` && (
          <JoinStep onBack={() => setStep({ kind: `choice` })} />
        )}
        {step.kind === `board` && (
          <BoardStep teamId={step.team.id} teamSlug={step.team.slug} />
        )}
      </div>
    </div>
  )
}

function ChoiceStep({
  onCreate,
  onJoin,
}: {
  onCreate: () => void
  onJoin: () => void
}) {
  return (
    <Card>
      <CardHeader className="text-center">
        <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-primary/10">
          <Users className="size-6 text-primary" />
        </div>
        <CardTitle className="text-xl">Welcome to Exponential</CardTitle>
        <CardDescription>
          Teams hold your boards and teammates. Create your own, or join one
          you&apos;ve been invited to.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <Button
          variant="outline"
          className="h-auto w-full justify-start gap-3 px-4 py-3 text-left"
          onClick={onCreate}
        >
          <Plus className="h-5 w-5 shrink-0 text-primary" />
          <span className="min-w-0">
            <span className="block font-medium">Create a team</span>
            <span className="block text-xs font-normal text-muted-foreground">
              Start fresh — you&apos;ll be the owner
            </span>
          </span>
        </Button>
        <Button
          variant="outline"
          className="h-auto w-full justify-start gap-3 px-4 py-3 text-left"
          onClick={onJoin}
        >
          <LinkIcon className="h-5 w-5 shrink-0 text-primary" />
          <span className="min-w-0">
            <span className="block font-medium">Join a team</span>
            <span className="block text-xs font-normal text-muted-foreground">
              Use an invite link a teammate sent you
            </span>
          </span>
        </Button>
      </CardContent>
    </Card>
  )
}

function CreateTeamStep({
  onBack,
  onCreated,
}: {
  onBack: () => void
  onCreated: (team: { id: string; slug: string }) => void
}) {
  const [name, setName] = useState(``)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // Plan-cap failures (free-tier owned-team cap) render as a softer nudge.
  const [limitError, setLimitError] = useState<string | null>(null)

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim()) return
    setSaving(true)
    setError(null)
    setLimitError(null)
    try {
      const { team } = await trpc.teams.create.mutate(
        { name: name.trim() },
        // The plan-limit case renders inline — the global mutation-error
        // toast would be redundant noise on top of it.
        { context: { skipErrorToast: true } }
      )
      onCreated({ id: team.id, slug: team.slug })
    } catch (err) {
      if (isPlanLimitError(err)) {
        setLimitError(err instanceof Error ? err.message : `Plan limit reached`)
      } else {
        setError(
          err instanceof Error ? err.message : `Failed to create team`
        )
      }
      setSaving(false)
    }
  }

  return (
    <Card>
      <CardHeader className="text-center">
        <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-primary/10">
          <Users className="size-6 text-primary" />
        </div>
        <CardTitle className="text-xl">Create a team</CardTitle>
        <CardDescription>
          Name your team — you can rename it and invite teammates later.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={handleCreate} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="onb-team-name">Team name</Label>
            <Input
              id="onb-team-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Acme Inc"
              autoFocus
            />
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

          <div className="flex items-center justify-between">
            <Button type="button" variant="ghost" onClick={onBack}>
              <ArrowLeft className="mr-2 h-4 w-4" />
              Back
            </Button>
            <Button type="submit" disabled={!name.trim() || saving}>
              {saving ? `Creating…` : `Create team`}
            </Button>
          </div>
        </form>
      </CardContent>
    </Card>
  )
}

function JoinStep({ onBack }: { onBack: () => void }) {
  const navigate = useNavigate()
  const [link, setLink] = useState(``)
  const [error, setError] = useState<string | null>(null)

  const handleContinue = (e: React.FormEvent) => {
    e.preventDefault()
    const token = extractInviteToken(link)
    if (!token) {
      setError(
        `That doesn't look like an invite link. It should look like ` +
          `${window.location.origin}/invite/…`
      )
      return
    }
    // The invite page handles acceptance; accepting marks onboarding
    // complete server-side, so this exits the wizard for good.
    void navigate({ to: `/invite/$token`, params: { token } })
  }

  return (
    <Card>
      <CardHeader className="text-center">
        <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-primary/10">
          <LinkIcon className="size-6 text-primary" />
        </div>
        <CardTitle className="text-xl">Join a team</CardTitle>
        <CardDescription>
          Ask a teammate for an invite link (team settings → Members), then
          paste it below.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={handleContinue} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="onb-invite-link">Invite link</Label>
            <Input
              id="onb-invite-link"
              value={link}
              onChange={(e) => {
                setLink(e.target.value)
                setError(null)
              }}
              placeholder={`${window.location.origin}/invite/…`}
              autoFocus
            />
          </div>

          {error && (
            <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {error}
            </div>
          )}

          <div className="flex items-center justify-between">
            <Button type="button" variant="ghost" onClick={onBack}>
              <ArrowLeft className="mr-2 h-4 w-4" />
              Back
            </Button>
            <Button type="submit" disabled={!link.trim()}>
              Continue
            </Button>
          </div>
        </form>
      </CardContent>
    </Card>
  )
}

// One form — name/prefix/icon/color and an optional repository. A repository
// is never required, which is what makes onboarding possible on instances
// without a GitHub App.
function BoardStep({
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
  )
}
