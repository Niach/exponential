import {
  BookOpen,
  Bug,
  Code2,
  Globe,
  Heart,
  Lightbulb,
  Megaphone,
  MessageCircle,
  Package,
  Rocket,
  Shield,
  SquareKanban,
  Star,
  Terminal,
  Wrench,
  Zap,
} from "lucide-react"
import type { LucideIcon } from "lucide-react"
import type { ProjectIcon, ProjectType } from "@exp/db-schema/domain"
import { projectIconValues } from "@exp/db-schema/domain"

// Creation templates (the old dev/tasks/feedback project TYPES survive only
// here): each pre-sets the public toggle, the stored icon, and whether the
// create form leads with the repo picker. Every resulting project is the same
// shape — repo optional, publicness a toggle. Native clients carry their own
// per-platform glyph mapping against the same curated icon names from the
// domain contract.
export type ProjectTemplate = {
  key: ProjectType
  label: string
  description: string
  icon: LucideIcon
  defaults: {
    isPublic: boolean
    icon: ProjectIcon
    // Whether the form opens with the repository section expanded (the repo
    // itself is always optional).
    suggestsRepo: boolean
  }
}

export const PROJECT_TEMPLATES: ProjectTemplate[] = [
  {
    key: `dev`,
    label: `Dev board`,
    description: `Connect a GitHub repo — branches, PRs, and coding sessions.`,
    icon: Code2,
    defaults: { isPublic: false, icon: `code`, suggestsRepo: true },
  },
  {
    key: `tasks`,
    label: `Task board`,
    description: `Plain issue tracking. No repository needed.`,
    icon: SquareKanban,
    defaults: { isPublic: false, icon: `square-kanban`, suggestsRepo: false },
  },
  {
    key: `feedback`,
    label: `Feedback board`,
    description: `Public: anyone with the link can read it. Collect feedback with the embeddable widget.`,
    icon: Megaphone,
    defaults: { isPublic: true, icon: `megaphone`, suggestsRepo: false },
  },
]

// Curated icon set (projectIconValues in the domain contract) → lucide
// components, in the picker's display order.
export const PROJECT_ICON_COMPONENTS: Record<ProjectIcon, LucideIcon> = {
  code: Code2,
  "square-kanban": SquareKanban,
  megaphone: Megaphone,
  bug: Bug,
  rocket: Rocket,
  "book-open": BookOpen,
  globe: Globe,
  heart: Heart,
  star: Star,
  zap: Zap,
  wrench: Wrench,
  shield: Shield,
  package: Package,
  terminal: Terminal,
  lightbulb: Lightbulb,
  "message-circle": MessageCircle,
}

export const PROJECT_ICON_OPTIONS = projectIconValues.map((name) => ({
  name,
  icon: PROJECT_ICON_COMPONENTS[name],
}))

// Resolve a project's display icon: the stored `icon` when set, else the
// legacy type-derived fallback (pre-collapse rows have icon = NULL).
export function getProjectIcon(project: {
  icon?: string | null
  type?: ProjectType | string | null
}): LucideIcon {
  if (project.icon && project.icon in PROJECT_ICON_COMPONENTS) {
    return PROJECT_ICON_COMPONENTS[project.icon as ProjectIcon]
  }
  switch (project.type) {
    case `tasks`:
      return SquareKanban
    case `feedback`:
      return Megaphone
    default:
      return Code2
  }
}
