import { ExponentialUiTree } from "@exp/ui"
import type { NestedNode } from "@exponential_app/eui"

import panel from "../../../ede/ui/panel.json"
import menu from "../../../ede/ui/menu.json"
import type { StyleguideEntry } from "./types.ts"

// #EDE-10 / #EDE-11: eDE's factory shell documents (apps/ede/ui), the genUI
// trees ede-shell draws as layer-shell surfaces on ede-compositor. The same
// JSON, the web renderer here, the gpui painter on desktop and in eDE itself
// (shots/ede-panel, shots/ede-panel-menu are the compositor's own captures).
// The `surface` header (where the shell puts it) is not part of the tree.

const PANEL = panel as unknown as { root: NestedNode; data: Record<string, unknown> }
const MENU = menu as unknown as { root: NestedNode }

export const entry: StyleguideEntry = {
  id: `ede-panel`,
  section: `special`,
  owner: `EDE-10`,
  title: `eDE panel`,
  blurb: `The desktop environment's factory panel (apps/ede/ui/panel.json): the launcher button, the taskbar from the window list and the clock in a 40 px top layer-shell surface; the menu (menu.json) is the panel's own popup surface. Agents and people edit these files; ede-ui validates, commits and rolls back (packages/ede). eDE-specific, not genUI: no new component, the core catalog only.`,
  status: {
    web: {
      state: `ok`,
      symbol: `ExponentialUiTree`,
      file: `packages/ui/src/exponential-ui-specimen.tsx`,
      note: `here only: the React renderer over apps/ede/ui/panel.json + menu.json (the app has no panel)`,
    },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/ede_panel.rs`, note: `the styleguide twin; the real thing is ede-shell (apps/ede/shell) on ede-compositor` },
    ios: { state: `n/a`, note: `eDE only: a desktop environment` },
    android: { state: `n/a`, note: `eDE only: a desktop environment` },
  },
  island: () => (
    <div className="flex flex-col gap-3">
      <span className="text-xs text-muted-foreground">panel.json · a top layer surface</span>
      <ExponentialUiTree id="ede-panel" node={PANEL.root} data={{ ...PANEL.data, time: `12:00`, windows: [{ id: `w1`, title: `Firefox`, focused: true }, { id: `w2`, title: `Terminal`, focused: false }] }} formFactor="wide" />
      <span className="text-xs text-muted-foreground">menu.json · the panel's popup surface</span>
      <div style={{ width: 280 }}>
        <ExponentialUiTree id="ede-menu" node={MENU.root} formFactor="wide" />
      </div>
    </div>
  ),
}
