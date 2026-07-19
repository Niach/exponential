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
// safety net derived from repo presence). Feeds both the display component
// below and the icon picker's selected value.
export function getProjectIconName(project: {
  icon?: string | null
  repositoryId?: string | null
}): ProjectIcon {
  if (project.icon && project.icon in PROJECT_ICON_COMPONENTS) {
    return project.icon as ProjectIcon
  }
  return project.repositoryId ? `code` : `square-kanban`
}

// Resolve a project's display icon component.
export function getProjectIcon(project: {
  icon?: string | null
  repositoryId?: string | null
}): LucideIcon {
  return PROJECT_ICON_COMPONENTS[getProjectIconName(project)]
}
