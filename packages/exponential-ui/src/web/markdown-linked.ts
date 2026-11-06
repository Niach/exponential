// #VAPP-258: the Markdown tree through a host's hooks, apart from `urls.ts`
// so the URL policy (which every primitive imports) never bundles the
// Markdown parser.

import type { AnyHostPlugin as HostPlugin } from "./host-types"
import { hostMarkdown, type MarkdownBlock } from "./markdown"
import { linkHref, textLinkSegments } from "./urls"

/** A Markdown's tree through the host's hooks (catalog/host.json
 *  `markdown`): attachments, image sizes, the text links and their chips
 *  (`fixtures/markdown-text-links.json`, `fixtures/markdown-host.json`).
 *  No hooks = the tree as parsed. */
export function linkedMarkdown(host: HostPlugin, tree: readonly MarkdownBlock[]): readonly MarkdownBlock[] {
  if (!host.textLinks && !host.imageSize && !host.attachment) return tree
  return hostMarkdown(tree, {
    link: (href) => linkHref(host, href),
    textLinks: host.textLinks ? (text) => textLinkSegments(host, text) : undefined,
    imageSize: host.imageSize,
    attachment: host.attachment,
  })
}
