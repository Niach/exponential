// FEED-131: the core's `search_matches` (SearchList, the Menu's searchable
// submenu): pure.

import { str } from "@exponential_app/ui/web"

/** Does a row match the query (case-insensitive, label or identifier)?
 *  The core's `search_matches`. */
export function searchMatches(item: { label?: unknown; identifier?: unknown }, query: string): boolean {
  const needle = query.trim().toLowerCase()
  return needle === `` || str(item.label).toLowerCase().includes(needle) || str(item.identifier).toLowerCase().includes(needle)
}
