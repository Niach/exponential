/* Libraries: each one Overview / Guides / Reference. */
import { LIBRARIES } from "../lib/libraries.ts"

export const LIBRARIES_GROUP = {
  label: `Libraries`,
  items: LIBRARIES.map((lib) => ({
    label: lib.name,
    collapsed: lib.slug !== `eui`,
    items: [
      { label: `Overview`, link: `/docs/libraries/${lib.slug}/` },
      { label: `Guides`, link: `/docs/libraries/${lib.slug}/guides/` },
      { label: `Reference`, link: `/docs/libraries/${lib.slug}/reference/` },
    ],
  })),
}
