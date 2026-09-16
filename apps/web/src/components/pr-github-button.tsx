import { conceptIcon, Button } from "@exp/ui"
import { contract } from "@exp/domain-contract"

// EXP-916: the GitHub control of a work HEADER — the PR page in a new tab, a
// ghost circle beside the Merge pill. The Changes face lost its own bar, so
// this is where a run's (or an issue's) pull request is reached from, on every
// face. The words are the contract's, so all four clients say them.

const GithubIcon = conceptIcon(`ui-github`)

export function PrGithubButton({ prUrl }: { prUrl: string }) {
  return (
    <Button
      variant="ghost"
      size="icon"
      className="size-9 shrink-0 rounded-full text-muted-foreground hover:text-foreground"
      aria-label={contract.diffUi.openOnGithub}
      title={contract.diffUi.openOnGithub}
      data-testid="changes-github-action"
      onClick={() => window.open(prUrl, `_blank`, `noopener,noreferrer`)}
    >
      <GithubIcon className="size-4" />
    </Button>
  )
}
