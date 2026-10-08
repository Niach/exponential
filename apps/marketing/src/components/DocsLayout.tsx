/* The docs layout lives in the shared site shell (@exp/site-shell, also worn
   by ui.exponential.at); this binds it to the marketing docs nav. */
import type { ComponentProps } from "react"
import { DocsLayout as SharedDocsLayout } from "@exp/site-shell"
import { DOCS_NAV } from "../lib/docs-nav"

export { DocsSection, DocsCode, DocsCallout, EnvVar } from "@exp/site-shell"

export function DocsLayout(
  props: Omit<ComponentProps<typeof SharedDocsLayout>, `nav`>
) {
  return <SharedDocsLayout nav={DOCS_NAV} {...props} />
}
