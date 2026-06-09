import { Bot, CircleDot, GitPullRequest, UserPlus } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import type { StepProps } from "./wizard"

const LOOP_STEPS = [
  {
    icon: CircleDot,
    label: `Create an issue`,
    detail: `Describe a task, bug, or idea — you just did.`,
  },
  {
    icon: UserPlus,
    label: `Assign it to your coding agent`,
    detail: `The agent drafts a plan for you to approve, then gets to work.`,
  },
  {
    icon: GitPullRequest,
    label: `Review the pull request`,
    detail: `It opens a PR on GitHub and you stay in control of the merge.`,
  },
] as const

// Closing screen: introduce the core loop (issue → agent → PR) and hand off to
// the "Set up coding agent" flow. Connecting GitHub and registering a desktop
// agent live there (and in the sidebar's setup checklist) — not in the wizard.
export function StepAgentIntro({ onNext, onSetupAgent }: StepProps) {
  return (
    <Card>
      <CardHeader className="text-center">
        <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-primary/10">
          <Bot className="size-6 text-primary" />
        </div>
        <CardTitle className="text-xl">Put a coding agent to work</CardTitle>
        <CardDescription>
          Exponential is built around one loop:
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-3">
          {LOOP_STEPS.map((step, i) => (
            <div key={step.label} className="flex items-start gap-3">
              <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-muted text-xs font-medium text-muted-foreground">
                {i + 1}
              </div>
              <div className="min-w-0">
                <div className="flex items-center gap-1.5 text-sm font-medium">
                  <step.icon className="size-3.5 text-muted-foreground" />
                  {step.label}
                </div>
                <p className="text-xs text-muted-foreground">{step.detail}</p>
              </div>
            </div>
          ))}
        </div>
        <div className="flex justify-between">
          <Button variant="ghost" onClick={onNext}>
            Maybe later
          </Button>
          {onSetupAgent && (
            <Button onClick={onSetupAgent}>Set up coding agent</Button>
          )}
        </div>
      </CardContent>
    </Card>
  )
}
