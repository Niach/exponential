import { Code2, Megaphone, SquareKanban } from "lucide-react"
import type { LucideIcon } from "lucide-react"
import type { ProjectType } from "@exp/db-schema/domain"

// The one place the three project types get their display identity (label,
// blurb, icon) on the web. Native clients carry their own per-platform icon
// mappings (SF Symbols / Material / gpui glyphs) against the same enum values
// from the domain contract.
export type ProjectTypeOption = {
  value: ProjectType
  label: string
  description: string
  icon: LucideIcon
}

export const PROJECT_TYPE_OPTIONS: ProjectTypeOption[] = [
  {
    value: `dev`,
    label: `Dev board`,
    description: `Backed by a GitHub repo — branches, PRs, and coding sessions.`,
    icon: Code2,
  },
  {
    value: `tasks`,
    label: `Task board`,
    description: `Plain issue tracking. No repository needed.`,
    icon: SquareKanban,
  },
  {
    value: `feedback`,
    label: `Feedback board`,
    description: `Public: anyone with the link can read it. Collect feedback with the embeddable widget.`,
    icon: Megaphone,
  },
]

export function getProjectTypeOption(
  type: ProjectType | string
): ProjectTypeOption {
  return (
    PROJECT_TYPE_OPTIONS.find((option) => option.value === type) ??
    PROJECT_TYPE_OPTIONS[0]
  )
}
