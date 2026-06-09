import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { Check } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Button } from "@/components/ui/button"
import { StepProject } from "@/components/onboarding/step-project"
import { StepFirstIssue } from "@/components/onboarding/step-first-issue"
import { StepAgentIntro } from "@/components/onboarding/step-agent-intro"
import { cn } from "@/lib/utils"

export type StepProps = {
  workspaceId: string
  workspaceSlug: string
  projectId?: string
  projectSlug?: string
  onProjectCreated?: (project: { id: string; slug: string }) => void
  onNext: () => void
  onSkip: () => void
  // Finish the wizard straight into the "Set up coding agent" flow (used by
  // the closing agent-intro step).
  onSetupAgent?: () => void
}

// Onboarding milestone = "first issue created" (a low bar). Name a project,
// create your first issue, then a closing screen that introduces the core
// loop (issue → agent → PR). Actual agent + GitHub setup stays in the
// separate "Set up coding agent" flow (the setup checklist).
const STEPS = [
  { id: `project`, title: `Project`, component: StepProject },
  { id: `issue`, title: `First issue`, component: StepFirstIssue },
  { id: `agent`, title: `Coding agent`, component: StepAgentIntro },
] as const

export function OnboardingWizard({
  workspaceId,
  workspaceSlug,
}: {
  workspaceId: string
  workspaceSlug: string
}) {
  const navigate = useNavigate()
  const [currentStep, setCurrentStep] = useState(0)
  const [project, setProject] = useState<{ id: string; slug: string } | null>(
    null
  )

  const finishWizard = async () => {
    await trpc.onboarding.complete.mutate()
    navigate({ to: `/w/$workspaceSlug`, params: { workspaceSlug } })
  }

  const finishToSetupAgent = async () => {
    await trpc.onboarding.complete.mutate()
    navigate({ to: `/w/$workspaceSlug/setup-agent`, params: { workspaceSlug } })
  }

  const handleNext = () => {
    if (currentStep >= STEPS.length - 1) {
      void finishWizard()
    } else {
      setCurrentStep((s) => s + 1)
    }
  }

  const ActiveComponent = STEPS[currentStep]?.component
  if (!ActiveComponent) return null

  return (
    <div className="flex min-h-screen items-center justify-center p-6">
      <div className="w-full max-w-2xl space-y-8">
        <div className="flex items-center justify-center gap-2">
          {STEPS.map((step, i) => (
            <div key={step.id} className="flex items-center gap-2">
              <div
                className={cn(
                  `flex h-7 w-7 items-center justify-center rounded-full text-xs font-medium transition-colors`,
                  i < currentStep && `bg-primary text-primary-foreground`,
                  i === currentStep &&
                    `bg-primary text-primary-foreground ring-2 ring-primary/30 ring-offset-2 ring-offset-background`,
                  i > currentStep && `bg-muted text-muted-foreground`
                )}
              >
                {i < currentStep ? <Check className="size-3.5" /> : i + 1}
              </div>
              {i < STEPS.length - 1 && (
                <div
                  className={cn(
                    `h-px w-6`,
                    i < currentStep ? `bg-primary` : `bg-muted`
                  )}
                />
              )}
            </div>
          ))}
        </div>

        <ActiveComponent
          workspaceId={workspaceId}
          workspaceSlug={workspaceSlug}
          projectId={project?.id}
          projectSlug={project?.slug}
          onProjectCreated={setProject}
          onNext={handleNext}
          onSkip={handleNext}
          onSetupAgent={() => void finishToSetupAgent()}
        />

        <div className="text-center">
          <Button
            variant="link"
            className="text-xs text-muted-foreground"
            onClick={() => void finishWizard()}
          >
            Skip setup entirely
          </Button>
        </div>
      </div>
    </div>
  )
}
