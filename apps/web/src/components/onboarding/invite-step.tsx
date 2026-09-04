import { useEffect, useState } from "react"
import { LoaderCircle, Sparkles } from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import { trpc } from "@/lib/trpc-client"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { ONBOARDING_COPY } from "@/components/onboarding/onboarding-copy"
import { StepCard, stepAdvanceLabel } from "@/components/onboarding/step-card"

const InviteIcon = conceptIcon(`ui-invite`)
const LinkIcon = conceptIcon(`editor-link`)
const CopyIcon = conceptIcon(`ui-copy`)
const CheckIcon = conceptIcon(`ui-check`)

// Step 3 of the first-run wizard (EXP-725): mint an invite link right after
// the first board, or skip. LINK-ONLY on every client (email invites stay in
// team settings), so the four steps read the same everywhere.
//
// Capacity comes from `teams.inviteCapacity` (members + PENDING invites vs
// seats; null = unlimited): at zero the button is disabled and, web only, the
// plan nudge explains why — the phones REMOVE the control instead (App Store
// 3.1.1). Nothing here reads the `team-invites` Electric shape: the team was
// created seconds ago, so every shape identity just rotated and the old
// long-poll lags; tRPC is the truth for this screen.
export function InviteStep({
  teamId,
  onNext,
}: {
  teamId: string
  onNext: () => void
}) {
  const [inviteUrl, setInviteUrl] = useState<string | null>(null)
  const [generating, setGenerating] = useState(false)
  const [copied, setCopied] = useState(false)
  const [remaining, setRemaining] = useState<number | null | undefined>(
    undefined
  )
  const [limitError, setLimitError] = useState<string | null>(null)

  const refreshCapacity = async () => {
    try {
      const { remaining } = await trpc.teams.inviteCapacity.query({ teamId })
      setRemaining(remaining)
    } catch {
      // The server still gates the mint; an unknown capacity keeps the
      // button usable rather than hiding it on a transient failure.
      setRemaining(null)
    }
  }

  useEffect(() => {
    void refreshCapacity()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [teamId])

  const handleGenerate = async () => {
    setGenerating(true)
    setLimitError(null)
    try {
      const { token } = await trpc.teamInvites.create.mutate(
        { teamId },
        // The plan-limit case renders inline; the global mutation-error
        // toast would be redundant noise on top of it.
        { context: { skipErrorToast: true } }
      )
      setInviteUrl(`${window.location.origin}/invite/${token}`)
      setCopied(false)
      void refreshCapacity()
    } catch (err) {
      if (isPlanLimitError(err)) {
        // Web keeps its billing copy (the server's "Your plan allows…").
        setLimitError(err instanceof Error ? err.message : `Plan limit reached`)
        setRemaining(0)
      } else {
        toast.error(`Couldn't create the invite`)
      }
    } finally {
      setGenerating(false)
    }
  }

  const handleCopy = async () => {
    if (!inviteUrl) return
    await navigator.clipboard.writeText(inviteUrl)
    setCopied(true)
    window.setTimeout(() => setCopied(false), 2_000)
  }

  const atCapacity = remaining === 0

  return (
    <StepCard
      icon={InviteIcon}
      title={ONBOARDING_COPY.invite.title}
      subtitle={ONBOARDING_COPY.invite.subtitle}
    >
      <div className="space-y-4 p-6">
        {inviteUrl && (
          <div className="flex items-center gap-2">
            <Input
              value={inviteUrl}
              readOnly
              className="text-xs font-mono"
              data-testid="invite-url-input"
            />
            <Button
              variant="outline"
              size="icon"
              onClick={() => void handleCopy()}
              className="shrink-0"
              aria-label={ONBOARDING_COPY.invite.copy}
              title={
                copied ? ONBOARDING_COPY.invite.copied : ONBOARDING_COPY.invite.copy
              }
            >
              {copied ? (
                <CheckIcon className="h-4 w-4" />
              ) : (
                <CopyIcon className="h-4 w-4" />
              )}
            </Button>
          </div>
        )}

        <Button
          variant="outline"
          onClick={() => void handleGenerate()}
          disabled={generating || atCapacity}
          data-testid="invite-generate"
        >
          {generating ? (
            <LoaderCircle className="mr-2 h-4 w-4 animate-spin" />
          ) : (
            <LinkIcon className="mr-2 h-4 w-4" />
          )}
          {ONBOARDING_COPY.invite.generate}
        </Button>

        {limitError && (
          <div className="flex items-start gap-2 rounded-md border border-primary/40 bg-primary/5 px-3 py-2 text-sm">
            <Sparkles className="mt-0.5 h-3.5 w-3.5 shrink-0 text-primary" />
            <span className="min-w-0 flex-1">{limitError}</span>
          </div>
        )}

        <div className="flex justify-end">
          <Button
            variant={inviteUrl ? `default` : `outline`}
            onClick={onNext}
            data-testid="onboarding-advance"
          >
            {stepAdvanceLabel(inviteUrl !== null, ONBOARDING_COPY.nav)}
          </Button>
        </div>
      </div>
    </StepCard>
  )
}
