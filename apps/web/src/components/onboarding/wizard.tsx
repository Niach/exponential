import { useEffect, useMemo, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { Check } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { getRuntimeConfig } from "@/lib/runtime-config"
import { Button } from "@/components/ui/button"
import { StepWorkspace } from "@/components/onboarding/step-workspace"
import { StepProject } from "@/components/onboarding/step-project"
import { StepPlan } from "@/components/onboarding/step-plan"
import { StepLabels } from "@/components/onboarding/step-labels"
import { StepAgent } from "@/components/onboarding/step-agent"
import { cn } from "@/lib/utils"

type StepDef = {
  id: string
  title: string
  component: React.ComponentType<StepProps>
  condition?: () => boolean
}

export type StepProps = {
  workspaceId: string
  workspaceSlug: string
  onNext: () => void
  onSkip: () => void
}

export function OnboardingWizard({
  workspaceId,
  workspaceSlug,
}: {
  workspaceId: string
  workspaceSlug: string
}) {
  const navigate = useNavigate()
  const [currentStep, setCurrentStep] = useState(0)
  const [isCloud, setIsCloud] = useState<boolean | null>(null)

  useEffect(() => {
    void getRuntimeConfig().then((config) => setIsCloud(config.isCloud))
  }, [])

  const allSteps: StepDef[] = useMemo(
    () => [
      { id: `workspace`, title: `Workspace`, component: StepWorkspace },
      { id: `project`, title: `Project`, component: StepProject },
      {
        id: `plan`,
        title: `Plan`,
        component: StepPlan,
        condition: () => isCloud === true,
      },
      { id: `labels`, title: `Labels`, component: StepLabels },
      { id: `agent`, title: `Agent`, component: StepAgent },
    ],
    [isCloud]
  )

  const steps = useMemo(
    () => allSteps.filter((s) => !s.condition || s.condition()),
    [allSteps]
  )

  const finishWizard = async () => {
    await trpc.onboarding.complete.mutate()
    navigate({
      to: `/w/$workspaceSlug`,
      params: { workspaceSlug },
    })
  }

  const handleNext = () => {
    if (currentStep >= steps.length - 1) {
      void finishWizard()
    } else {
      setCurrentStep((s) => s + 1)
    }
  }

  const handleSkip = () => {
    handleNext()
  }

  if (isCloud === null) return null

  const ActiveComponent = steps[currentStep]?.component
  if (!ActiveComponent) return null

  return (
    <div className="flex min-h-screen items-center justify-center p-6">
      <div className="w-full max-w-lg space-y-8">
        <div className="flex items-center justify-center gap-2">
          {steps.map((step, i) => (
            <div key={step.id} className="flex items-center gap-2">
              <div
                className={cn(
                  `flex h-7 w-7 items-center justify-center rounded-full text-xs font-medium transition-colors`,
                  i < currentStep &&
                    `bg-primary text-primary-foreground`,
                  i === currentStep &&
                    `bg-primary text-primary-foreground ring-2 ring-primary/30 ring-offset-2 ring-offset-background`,
                  i > currentStep &&
                    `bg-muted text-muted-foreground`
                )}
              >
                {i < currentStep ? (
                  <Check className="size-3.5" />
                ) : (
                  i + 1
                )}
              </div>
              {i < steps.length - 1 && (
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
          onNext={handleNext}
          onSkip={handleSkip}
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
