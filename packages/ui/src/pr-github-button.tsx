import { conceptIcon } from "./icons.generated"
import { Button } from "./button"
import { contract } from "@exp/domain-contract"
import { cn } from "./cn"
import { FabButton } from "./fab-chrome"

// EXP-916: THE GitHub control of every diff surface — the PR page in a new
// tab. One component, three shapes, so the words (the contract's) and the
// behaviour are written once:
//
//   ghost  — the work HEADER's action slot, beside the Merge pill
//   circle — a phone work BAR slot (a run with no issue header to hang it on)
//   glass  — the Reviews header's action row, beside Close PR and Merge PR
//
// Each shape keeps the `data-testid` the surface had before they were merged;
// `testId` overrides it for a surface that needs its own.

const GithubIcon = conceptIcon(`ui-github`)

/** Which chrome the button wears — see the header comment. */
export type PrGithubVariant = `ghost` | `circle` | `glass`

const DEFAULT_TEST_ID: Record<PrGithubVariant, string> = {
  ghost: `changes-github-action`,
  circle: `changes-github-circle`,
  glass: `changes-github-link`,
}

export function PrGithubButton({
  prUrl,
  variant = `ghost`,
  testId,
  className,
}: {
  prUrl: string
  variant?: PrGithubVariant
  testId?: string
  className?: string
}) {
  const label = contract.diffUi.openOnGithub
  const open = () => window.open(prUrl, `_blank`, `noopener,noreferrer`)
  const testid = testId ?? DEFAULT_TEST_ID[variant]

  // The bar circle is the 52px glass slot every other bar control is (the file
  // sheet, the face switcher) — `FabButton` IS that slot (EXP-962), at the
  // secondary emphasis its siblings wear.
  if (variant === `circle`) {
    return (
      <FabButton
        aria-label={label}
        title={label}
        data-testid={testid}
        onClick={open}
        className={className}
      >
        <GithubIcon className="size-5" />
      </FabButton>
    )
  }

  const glass = variant === `glass`
  return (
    <Button
      variant={glass ? `glass` : `ghost`}
      size={glass ? `icon-sm` : `icon`}
      className={cn(
        `shrink-0 rounded-full`,
        glass ? undefined : `size-9 text-muted-foreground hover:text-foreground`,
        className
      )}
      aria-label={label}
      title={label}
      data-testid={testid}
      onClick={open}
    >
      <GithubIcon className="size-4" />
    </Button>
  )
}
