import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import {
  ArrowLeft,
  Github,
  Link as LinkIcon,
  Plus,
  Sparkles,
  Users,
  X,
} from "lucide-react"
import type { BoardIcon } from "@exp/db-schema/domain"
import { trpc } from "@/lib/trpc-client"
import {
  conceptIcon,
  BrandHeading,
  Button,
  Pill,
  Input,
  Label,
  GlassGroup,
} from "@exp/ui"
import { useSession } from "@/hooks/use-session"
import { useSignOut } from "@/hooks/use-sign-out"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import { useCreateBoard } from "@/hooks/use-create-board"
import {
  BoardIdentityRow,
  BoardPrefixField,
} from "@/components/board-form-fields"
import {
  GithubRepoPicker,
  type PickerRepo,
  useGithubConnectShortcut,
} from "@/components/github-repo-picker"
import { derivePrefix } from "@/lib/board"
import { ONBOARDING_COPY } from "@/components/onboarding/onboarding-copy"
import { StepCard } from "@/components/onboarding/step-card"
import { InviteStep } from "@/components/onboarding/invite-step"
import { DevicesStep } from "@/components/onboarding/devices-step"

const BoardsIcon = conceptIcon(`nav-boards`)

// Onboarding (EXP-188): signups get no team anymore, so the wizard is a
// step machine. EXP-725 made it the SAME four steps on every client:
// choice → create-team | join → board → invite → devices. `initialTeam`
// (resolved by the route via teams.getDefault) skips straight to the board
// step: the resumed-onboarding case (team exists but onboarding never
// completed). The join path leaves the wizard for the invite page: accepting
// marks onboarding complete server-side, and EXP-1169 gives a joiner who owns
// no device the SAME devices step there (`routes/invite/$token.tsx`), so both
// paths end on it. The board step stamps completion (as before); invite and
// devices continue client-side, are skippable, and the getting-started
// checklist covers whatever was skipped after a reload.
type WizardTeam = { id: string; slug: string }
type WizardStep =
  | { kind: `choice` }
  | { kind: `create-team` }
  | { kind: `join` }
  | { kind: `board`; team: WizardTeam }
  | { kind: `invite`; team: WizardTeam }
  | { kind: `devices`; team: WizardTeam }

/** Steps a resumed wizard may be told to start at (`?step=`, the shots
 * pipeline's capture hook — see routes/_authenticated/onboarding.tsx). */
export type WizardEntryStep = `invite` | `devices`

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
  initialStep,
}: {
  initialTeam: WizardTeam | null
  initialStep?: WizardEntryStep
}) {
  const navigate = useNavigate()
  const [step, setStep] = useState<WizardStep>(
    initialTeam
      ? { kind: initialStep ?? `board`, team: initialTeam }
      : { kind: `choice` }
  )

  return (
    <WizardFrame>
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
        <BoardStep
          teamId={step.team.id}
          onCreated={() => setStep({ kind: `invite`, team: step.team })}
        />
      )}
      {step.kind === `invite` && (
        <InviteStep
          teamId={step.team.id}
          onNext={() => setStep({ kind: `devices`, team: step.team })}
        />
      )}
      {step.kind === `devices` && (
        <DevicesStep
          teamId={step.team.id}
          onNext={() =>
            void navigate({
              to: `/t/$teamSlug`,
              params: { teamSlug: step.team.slug },
            })
          }
        />
      )}
    </WizardFrame>
  )
}

/** The page chrome every step sits in: centred, one column, the sign-out
 * escape underneath. Shared with the join step on the invite page. */
export function WizardFrame({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex min-h-screen items-center justify-center p-6">
      <div className="w-full max-w-2xl">
        {children}
        <SignedInFooter />
      </div>
    </div>
  )
}

// The wizard is the ONLY surface a team-less user can reach, and it has no
// sidebar (so no user menu). Without an escape, the wrong account (a second
// login in the same browser) strands them on "Welcome to Exponential" with
// create-or-join as the only options. Mirrors the phones' persistent
// sign-out (`OnboardingView.swift`, MOBILE_ONBOARDING_COPY.signOut): shown
// under every step, muted, never inside a step's error state.
function SignedInFooter() {
  const { data: session } = useSession()
  const handleSignOut = useSignOut()
  const email = session?.user?.email
  // The GitHub install redirect can land a signed-out person in this frame
  // (routes/integrations/github.tsx): nobody to sign out, so no row.
  if (!email) return null
  return (
    <div className="mt-4 flex items-center justify-center gap-1 text-xs text-muted-foreground">
      {/* ×4: muted "Signed in as {email} · Sign out". */}
      <span className="min-w-0 truncate" data-testid="onboarding-signed-in-as">
        Signed in as {email}
      </span>
      <span aria-hidden>·</span>
      <Button
        type="button"
        variant="link"
        size="sm"
        className="h-auto p-0 text-xs font-normal text-muted-foreground hover:text-foreground"
        onClick={() => void handleSignOut()}
      >
        Sign out
      </Button>
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
  // EXP-1176: the same frame as the login page. The mark over the title,
  // two standard buttons, no card and no line explaining what a team is.
  return (
    <div className="mx-auto w-full max-w-sm space-y-6">
      <BrandHeading title="Welcome to Exponential" />
      <div className="space-y-2.5">
        <Button variant="outline" className="w-full" onClick={onCreate}>
          <Plus />
          Create a team
        </Button>
        <Button variant="outline" className="w-full" onClick={onJoin}>
          <LinkIcon />
          Join a team
        </Button>
      </div>
    </div>
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
    <StepCard
      icon={Users}
      title="Create a team"
    >
      <div className="p-6">
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
      </div>
    </StepCard>
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
    // The invite page handles acceptance (and the joiner's devices step);
    // accepting marks onboarding complete server-side, so this exits the
    // wizard for good.
    void navigate({ to: `/invite/$token`, params: { token } })
  }

  return (
    <StepCard
      icon={LinkIcon}
      title="Join a team"
    >
      <div className="p-6">
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
              placeholder="Paste an invite link"
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
      </div>
    </StepCard>
  )
}

// One form — name/prefix/icon/color and an optional repository. A repository
// is never required, which is what makes onboarding possible on instances
// without a GitHub App. Creating the board stamps onboarding complete
// (server-side, once); the wizard then continues to the invite step.
function BoardStep({
  teamId,
  onCreated,
}: {
  teamId: string
  onCreated: () => void
}) {
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
  const connectShortcut = useGithubConnectShortcut(teamId)

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
      onCreated()
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
    <StepCard
      icon={BoardsIcon}
      title={ONBOARDING_COPY.board.title}
      subtitle={ONBOARDING_COPY.board.subtitle}
    >
      <div className="space-y-4 p-6">
        {/* EXP-862: the board form is ONE glass group on every client —
            identity (icon, colour, name) in one row, then the prefix. */}
        <GlassGroup>
          <BoardIdentityRow
            name={name}
            onNameChange={handleNameChange}
            autoFocus
            icon={icon}
            onIconChange={setIcon}
            color={color}
            onColorChange={setColor}
          />
          <BoardPrefixField value={prefix} onChange={setPrefix} />
        </GlassGroup>

        <div className="space-y-2 border-t pt-4">
          <Label>Repository (optional)</Label>
          {repo ? (
            <div className="flex items-center gap-2 rounded-md border px-3 py-2 text-sm">
              <Github className="h-4 w-4 shrink-0 text-muted-foreground" />
              <span className="min-w-0 flex-1 truncate">
                {repo.fullName}
              </span>
              <Pill type="button" mode="action" onClick={() => setRepo(null)}>
                <X />
                Change
              </Pill>
            </div>
          ) : showRepo ? (
            <GithubRepoPicker teamId={teamId} onSelect={setRepo} />
          ) : (
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="w-full justify-start text-muted-foreground"
              onClick={() => {
                // One-step connect (EXP-390): open the GitHub popup directly
                // when no account is linked; the expanded picker is the
                // return surface either way.
                connectShortcut()
                setShowRepo(true)
              }}
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
            {saving ? `Creating…` : ONBOARDING_COPY.board.create}
          </Button>
        </div>
      </div>
    </StepCard>
  )
}
