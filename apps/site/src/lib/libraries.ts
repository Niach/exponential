/* The Exponential SDK's modules: one product page each (/sdk/<slug>/) and one
   docs group (/docs/sdk/<slug>/: Overview / Guides / Reference). */
export interface Library {
  slug: string
  name: string
  /** The product page path. */
  product: string
  /** The old site's pages this module's docs absorb (docs/site/audit.md). */
  from: string
}

export const LIBRARIES: Library[] = [
  { slug: `genui`, name: `genUI`, product: `/sdk/genui/`, from: `/ui/` },
  { slug: `motion`, name: `Motion`, product: `/sdk/motion/`, from: `/ui/guides/motion/` },
  { slug: `voice`, name: `Voice`, product: `/sdk/voice/`, from: `/talk/` },
  { slug: `p2p`, name: `P2P`, product: `/sdk/p2p/`, from: `/net/` },
  { slug: `computer-use`, name: `Computer Use`, product: `/sdk/computer-use/`, from: `/pilot/` },
  { slug: `mcp-proxy`, name: `MCP Proxy`, product: `/sdk/mcp-proxy/`, from: `/mcp/` },
  { slug: `agents`, name: `Agents`, product: `/sdk/agents/`, from: `/agent/` },
  { slug: `core`, name: `Core`, product: `/sdk/core/`, from: `/core/` },
  { slug: `code`, name: `Code`, product: `/sdk/code/`, from: `/code/` },
]
