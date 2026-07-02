import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { Check } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Button } from "@/components/ui/button"
import { StepProject } from "@/components/onboarding/step-project"
import { StepGithub } from "@/components/onboarding/step-github"
import { cn } from "@/lib/utils"

export type StepProps = {
  workspaceId: string
  workspaceSlug: string
  projectId?: string
  projectSlug?: string
  onProjectCreated?: (project: { id: string; slug: string }) => void
  onNext: () => void
  onSkip: () => void
}

// Onboarding = name a project, then connect GitHub so "Start coding" works
// from day one — then finish into the workspace. Both steps are skippable.
const STEPS = [
  { id: `project`, title: `Project`, component: StepProject },
  { id: `github`, title: `GitHub`, component: StepGithub },
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
