// VAPP-218 (eApps U1, apps.json `screens`): the web half of the preview's
// screens. The route `/t/$teamSlug/apps/$appSlug/$screen` carries the
// request; its query = the screen's params. The router parses a query value
// as JSON (`x=1` → the number 1), so the route's search is read back as
// strings before the contract's `screenParamsFromQuery` drops the editor's
// own keys.

import { screenParamsFromQuery } from "@exp/domain-contract/apps"

/** A screen the editor's preview is asked to show (the route's, else none). */
export interface ScreenRequest {
  screen: string
  params: Record<string, string>
}

/** The router's parsed search → the screen's params (strings, sorted, the
 *  editor's `tab`/`frame` dropped). */
export function routeScreenParams(search: Record<string, unknown>): Record<string, string> {
  const strings: Record<string, string> = {}
  for (const [key, value] of Object.entries(search)) {
    if (value === undefined) continue
    strings[key] = typeof value === `string` ? value : JSON.stringify(value)
  }
  return screenParamsFromQuery(strings)
}
