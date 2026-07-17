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
import type { ProjectIcon } from "@exp/db-schema/domain"
import { projectIconValues } from "@exp/db-schema/domain"

// Creation templates (the old dev/tasks/feedback project TYPES survive only
// here): each pre-sets the public toggle, the stored icon, and whether the
// create form leads with the repo picker. Every resulting project is the same
// shape — repo optional, publicness a toggle. Native clients carry their own
// per-platform glyph mapping against the same curated icon names from the
// domain contract.
export type ProjectTemplate = {
  key: `dev` | `tasks` | `feedback`
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

// Resolve a project's canonical icon NAME: the stored `icon` when set (the
// drop-type migration backfilled every row, so the fallback is a cosmetic
// safety net derived from publicness/repo presence). Feeds both the display
// component below and the icon picker's selected value.
export function getProjectIconName(project: {
  icon?: string | null
  isPublic?: boolean
  repositoryId?: string | null
}): ProjectIcon {
  if (project.icon && project.icon in PROJECT_ICON_COMPONENTS) {
    return project.icon as ProjectIcon
  }
  if (project.isPublic) return `megaphone`
  return project.repositoryId ? `code` : `square-kanban`
}

// Resolve a project's display icon component.
export function getProjectIcon(project: {
  icon?: string | null
  isPublic?: boolean
  repositoryId?: string | null
}): LucideIcon {
  return PROJECT_ICON_COMPONENTS[getProjectIconName(project)]
}
