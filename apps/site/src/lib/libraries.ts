/* The open libraries: one product page each (/<path>/) and one docs group
   (/docs/libraries/<slug>/: Overview / Guides / Reference). */
export interface Library {
  slug: string
  name: string
  /** The product page path. */
  product: string
  /** The old site's pages this library's docs absorb (docs/site/audit.md). */
  from: string
}

export const LIBRARIES: Library[] = [
  { slug: `eui`, name: `eUI`, product: `/ui/`, from: `/ui/` },
  { slug: `motion`, name: `Motion`, product: `/motion/`, from: `/ui/guides/motion/` },
  { slug: `etalk`, name: `eTalk`, product: `/talk/`, from: `/talk/` },
  { slug: `enet`, name: `eNet`, product: `/net/`, from: `/net/` },
  { slug: `epilot`, name: `ePilot`, product: `/pilot/`, from: `/pilot/` },
  { slug: `emcp`, name: `eMCP`, product: `/mcp/`, from: `/mcp/` },
  { slug: `eagent`, name: `eAgent`, product: `/agent/`, from: `/agent/` },
  { slug: `ecore`, name: `eCore`, product: `/core/`, from: `/core/` },
]
