import { REPO_PAGE_COPY } from "@exp/domain-contract/repo-page"

// #EXP-1616 (repo-page.json `copy.checks`): a checks rollup's Chip{dot}:
// its label and its dot in the theme's semantic colour.
const TONE: Record<string, string> = {
  green: `var(--color-emerald-400)`,
  amber: `var(--color-amber-400)`,
  red: `var(--destructive)`,
}

type CheckState = keyof typeof REPO_PAGE_COPY.checks

export function checksDotColor(state: string): string {
  return TONE[REPO_PAGE_COPY.checks[state as CheckState]?.tone ?? ``] ?? `var(--muted-foreground)`
}

export function checksLabel(state: string): string {
  return REPO_PAGE_COPY.checks[state as CheckState]?.label ?? state
}
