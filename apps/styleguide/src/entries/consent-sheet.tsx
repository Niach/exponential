import fixture from "@exp/eengine/fixtures/consent.json"
import { consentSheet, manageSheet, type SurfaceComponent } from "@exp/eengine/consent"
import type { ManifestEntry, Grants } from "@exp/eengine/capabilities"
import { ExponentialUiTree } from "@exp/ui"
import type { NestedNode } from "@exponential_app/eui"

import type { StyleguideEntry } from "./types.ts"

// VAPP-262 (Engine E2, consent.json, docs/spec/apps.md §13.2): the Engine's
// default ConsentUi, two genUI surfaces from existing components only. The
// trees are `consentSheet` / `manageSheet`'s own output over the fixture's
// first sheet and manage cases, painted by THE React renderer (the desktop
// twin paints the fixture's `expected`, the same components by the replay).

const SHEET = fixture.sheetCases[0]!
const MANAGE = fixture.manageCases[0]!

/** The frames the sheets hang in (the desktop twin's viewports). */
const ASK_HEIGHT = 440
const MANAGE_HEIGHT = 300

const NODE_KEYS = new Set([`id`, `component`, `children`, `slots`, `on`, `style`, `visible`, `accessibility`])

/** A flat A2UI list (root = `root`) as the nested tree the renderer takes. */
function nest(components: readonly SurfaceComponent[]): NestedNode {
  const byId = new Map(components.map((c) => [c.id, c]))
  const node = (id: string): NestedNode => {
    const c = byId.get(id)!
    const props = Object.fromEntries(Object.entries(c).filter(([k]) => !NODE_KEYS.has(k)))
    const children = (c.children as string[] | undefined)?.map(node)
    const slots = c.slots ? Object.fromEntries(Object.entries(c.slots as Record<string, string>).map(([k, v]) => [k, node(v)])) : undefined
    return { id, component: c.component, props, ...(c.on ? { on: c.on as NestedNode[`on`] } : {}), ...(children ? { children } : {}), ...(slots ? { slots } : {}) }
  }
  return node(`root`)
}

const ASK = consentSheet(SHEET.app, SHEET.entry as ManifestEntry)
const ASK_NODE = nest(ASK.components)
const MANAGE_NODE = nest(manageSheet(MANAGE.manifest as ManifestEntry[], MANAGE.grants as Grants))

export const entry: StyleguideEntry = {
  id: `consent-sheet`,
  section: `special`,
  owner: `VAPP-262`,
  title: `Consent sheet`,
  blurb: `The Engine's default ConsentUi (consent.json, docs/spec/apps.md §13.2), the same genUI surface on every host and no new primitive. When the broker answers ask: a bottom Drawer "${fixture.copy.title}", the manifest's why as Text, ChoiceTiles (one column) ${fixture.copy.options.map((o) => o.label).join(` · `)} starting on ${fixture.copy.options[0]!.label}, and the ${fixture.copy.confirm} Button sending consent.answer; closing the Drawer answers dismiss. ${fixture.copy.manageTitle} (the app's … menu): a Drawer with one Switch per declared ask scope in manifest order (the capability's label, the why, checked = granted); off revokes, so the next call asks again. A host may replace the painter, never the rule.`,
  status: {
    web: { state: `ok`, symbol: `consentSheet / manageSheet`, file: `packages/eengine/src/consent.ts`, note: `here: ExponentialUiTree (packages/ui) over the reference's output` },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/consent_sheet.rs`, note: `the gpui painter over consent.json's expected trees` },
    ios: { state: `ok`, symbol: `Drawer / ChoiceTiles / Button / Switch`, file: `packages/exponential-ui-swift/Sources/ExponentialUI` },
    android: { state: `ok`, symbol: `Drawer / ChoiceTiles / Button / Switch`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose` },
  },
  island: () => (
    <div className="flex flex-col gap-3">
      <span className="text-xs text-muted-foreground">{`${SHEET.app} · ${SHEET.entry.id}`}</span>
      <div style={{ width: 480, maxWidth: `100%` }}>
        <ExponentialUiTree id="consent-sheet-ask" node={ASK_NODE} data={ASK.data} inlineOverlays frame={ASK_HEIGHT} />
      </div>
      <span className="text-xs text-muted-foreground">{fixture.copy.manageTitle}</span>
      <div style={{ width: 480, maxWidth: `100%` }}>
        <ExponentialUiTree id="consent-sheet-manage" node={MANAGE_NODE} inlineOverlays frame={MANAGE_HEIGHT} />
      </div>
    </div>
  ),
}
