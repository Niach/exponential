import type { LucideIcon } from "lucide-react"
import { GlassGroup } from "@/components/ui/glass-rows"

// The wizard card every step shares (EXP-698 r6 style contract: this is the
// desktop IDE's reference, `onboarding.rs` mirrors the chrome): a primary-
// tinted disc with the step's concept icon, the title, a muted subtitle,
// then the body section.
export function StepCard({
  icon: Icon,
  title,
  subtitle,
  children,
}: {
  icon: LucideIcon
  title: string
  subtitle: string
  children: React.ReactNode
}) {
  return (
    <GlassGroup>
      <div className="flex flex-col gap-1.5 p-6 text-center">
        <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-primary/10">
          <Icon className="size-6 text-primary" />
        </div>
        <h2 className="text-xl font-semibold">{title}</h2>
        <p className="text-sm text-muted-foreground">{subtitle}</p>
      </div>
      {children}
    </GlassGroup>
  )
}

/** The trailing wizard button for a skippable step (EXP-725, x4 contract):
 * `Skip for now` until the step produced something, `Continue` once it did.
 * ONE button, so e2e and the capture recipes have a deterministic target. */
export function stepAdvanceLabel(
  produced: boolean,
  nav: { skip: string; continue: string }
): string {
  return produced ? nav.continue : nav.skip
}
