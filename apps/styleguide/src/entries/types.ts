import type { ReactElement } from "react"

import type { ComponentPlatform, ComponentStatus } from "../components.tsx"
import type { SectionId } from "../sections/index.ts"

/**
 * EXP-1029 contract — one styleguide entry under the four sections.
 *
 * The same halves as `ComponentSpec` (a title, a blurb, the per-platform
 * status table, and EITHER a hand-written `render` OR a real `@exp/ui`
 * `island`) plus where it lives (`section`) and who fills it (`owner`). A
 * `placeholder` entry renders one line saying so; the leaf that owns it
 * replaces the body and drops the flag.
 */
export interface StyleguideEntry {
  id: string
  section: SectionId
  /** The issue that fills this entry. */
  owner: string
  title: string
  blurb: string
  /** Absent on a placeholder; the leaf fills the table with its files. */
  status?: Record<ComponentPlatform, ComponentStatus>
  placeholder?: true
  render?: () => string
  island?: () => ReactElement
}
