import { ExponentialUiTree } from "@exp/ui"
import type { NestedNode } from "@exponential_app/eui"

import panel from "../../../ede/ui/panel.json"
import menu from "../../../ede/ui/menu.json"
import switcher from "../../../ede/ui/switcher.json"
import osd from "../../../ede/ui/osd.json"
import notifications from "../../../ede/ui/notifications.json"
import center from "../../../ede/ui/notification-center.json"
import trayMenu from "../../../ede/ui/tray-menu.json"
import settings from "../../../ede/ui/settings.json"
import type { StyleguideEntry } from "./types.ts"

// #EDE-10 / #EDE-11 / #EDE-12: eDE's factory shell documents (apps/ede/ui),
// the genUI trees ede-shell draws as layer-shell surfaces on ede-compositor.
// The same JSON, the web renderer here, the gpui painter on desktop and in
// eDE itself (shots/ede-panel, ede-panel-menu, ede-switcher, ede-osd are the
// compositor's own captures). The `surface` header (where the shell puts it,
// which host functions it may call) is not part of the tree.

const PANEL = panel as unknown as { root: NestedNode; data: Record<string, unknown> }
const MENU = menu as unknown as { root: NestedNode }
const SWITCHER = switcher as unknown as { root: NestedNode }
const OSD = osd as unknown as { root: NestedNode }
const NOTIFICATIONS = notifications as unknown as { root: NestedNode }
const CENTER = center as unknown as { root: NestedNode }
const TRAY_MENU = trayMenu as unknown as { root: NestedNode }
const SETTINGS = settings as unknown as { root: NestedNode; data: Record<string, unknown> }

const SAMPLE_SERVICES = {
  network: { available: true, wifiEnabled: true, devices: [{ path: `/dev/1`, interface: `wlp3s0`, state: `connected`, connection: `Home` }], accessPoints: [{ ssid: `Home`, strength: 82, active: true }, { ssid: `Cafe Nero`, strength: 47, active: false }] },
  sound: { available: true, volume: 40, muted: false, micVolume: 100, sinks: [{ id: 55, name: `Built-in Audio Analog Stereo`, volume: 40, default: true }] },
  bluetooth: { available: true, powered: true, devices: [{ path: `/bt/1`, name: `Keyboard K380`, address: `F4:73:35:12:AB:01`, connected: true }] },
  power: { available: true, batteryText: `73% · discharging`, profile: `balanced` },
  updates: { available: true, countText: `2`, packages: [{ name: `firefox`, from: `129.0`, version: `130.0` }] },
  printers: { available: true, printers: [{ name: `HP_LaserJet`, state: `idle`, default: true }] },
  about: { available: true, hostname: `ubuntu-wayland`, os: `Ubuntu 26.04 LTS`, kernel: `7.0.0-38-generic`, cpu: `13th Gen Intel Core i9-13900K`, memoryText: `7.2 GB`, ede: `0.1.0` },
}
const SAMPLE_OUTPUTS = [{ name: `HEADLESS-1`, summary: `Exponential eDE headless · 1280×720 at 1× (0, 0)`, current: `1280x720@60`, modeOptions: [{ label: `1280x720@60 (preferred)`, value: `1280x720@60` }], scale: 1, scaleText: `1` }]

const SAMPLE_NOTIFICATIONS = {
  toasts: [{ id: 1, app: `Thunderbird`, summary: `Release notes for 1.4`, body: `Danny: the eDE M1 demo runs on Friday, bring the VM.`, icon: ``, actions: [{ key: `open`, label: `Open` }, { key: `archive`, label: `Archive` }], urgency: 1, at: `11:58` }],
  list: [
    { id: 1, app: `Thunderbird`, summary: `Release notes for 1.4`, body: `Danny: the eDE M1 demo runs on Friday, bring the VM.`, icon: ``, actions: [], urgency: 1, at: `11:58` },
    { id: 2, app: `Software`, summary: `Updates installed`, body: `12 packages were updated.`, icon: ``, actions: [], urgency: 0, at: `11:40` },
  ],
  count: 2,
}
const SAMPLE_TRAY_MENU = { service: `demo/network`, title: `Network`, items: [
  { id: 1, label: `Connect to Wi-Fi`, enabled: true, separator: false, checked: false, checkable: false, depth: 0 },
  { id: 2, label: `Airplane mode`, enabled: true, separator: false, checked: false, checkable: true, depth: 0 },
  { id: 3, label: ``, enabled: true, separator: true, checked: false, checkable: false, depth: 0 },
  { id: 4, label: `Settings…`, enabled: true, separator: false, checked: false, checkable: false, depth: 0 },
] }

const SAMPLE_APPS = [
  { id: `org.gnome.Calculator.desktop`, name: `Calculator`, comment: `Perform arithmetic, scientific or financial calculations`, exec: `gnome-calculator`, icon: `` },
  { id: `nemo.desktop`, name: `Files`, comment: `Access and organize files`, exec: `nemo`, icon: `` },
  { id: `firefox.desktop`, name: `Firefox`, comment: `Browse the web`, exec: `firefox`, icon: `` },
  { id: `terminal.desktop`, name: `Terminal`, comment: `Use the command line`, exec: `x-terminal-emulator`, icon: `` },
]
const SAMPLE_SWITCHER = { windows: [
  { id: `w1`, title: `Firefox`, selected: false, thumbnail: `` },
  { id: `w2`, title: `Terminal`, selected: true, thumbnail: `` },
  { id: `w3`, title: `Files`, selected: false, thumbnail: `` },
] }

export const entry: StyleguideEntry = {
  id: `ede-panel`,
  section: `special`,
  owner: `EDE-10`,
  title: `eDE panel`,
  blurb: `The desktop environment's factory shell documents: the panel (apps/ede/ui/panel.json: the launcher button, the taskbar from the window list and the clock in a 40 px top layer-shell surface), the launcher menu (menu.json, the panel's own popup surface: search, the installed apps, the power row), the Alt+Tab switcher (switcher.json, an on-demand overlay with live thumbnails), the OSD (osd.json, the media keys' level), the toasts of the shell's own notification server (notifications.json), the bell's notification centre (notification-center.json), a tray item's dbusmenu (tray-menu.json) and the settings surface (settings.json, a page per domain over ede-services). Agents and people edit these files; ede-ui validates, commits and rolls back (packages/ede). eDE-specific, not genUI: no new component, the core catalog only.`,
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
      <ExponentialUiTree id="ede-panel" node={PANEL.root} data={{ ...PANEL.data, time: `12:00`, windows: [{ id: `w1`, title: `Firefox`, focused: true }, { id: `w2`, title: `Terminal`, focused: false }], tray: [{ service: `demo/network`, title: `Network`, status: `Active`, icon: `` }], notifications: { count: 2 } }} formFactor="wide" />
      <span className="text-xs text-muted-foreground">menu.json · the panel's popup surface (the launcher)</span>
      <div style={{ width: 360 }}>
        <ExponentialUiTree id="ede-menu" node={MENU.root} data={{ query: ``, apps: SAMPLE_APPS }} formFactor="wide" />
      </div>
      <span className="text-xs text-muted-foreground">switcher.json · Alt+Tab, an on-demand overlay</span>
      <div style={{ width: 760 }}>
        <ExponentialUiTree id="ede-switcher" node={SWITCHER.root} data={{ switcher: SAMPLE_SWITCHER }} formFactor="wide" />
      </div>
      <span className="text-xs text-muted-foreground">osd.json · the media keys' level</span>
      <div style={{ width: 280 }}>
        <ExponentialUiTree id="ede-osd" node={OSD.root} data={{ osd: { icon: `media-volume-down`, level: 45, label: `45%` } }} formFactor="wide" />
      </div>
      <span className="text-xs text-muted-foreground">notifications.json · toasts, an on-demand top-right surface</span>
      <div style={{ width: 380 }}>
        <ExponentialUiTree id="ede-notifications" node={NOTIFICATIONS.root} data={{ notifications: SAMPLE_NOTIFICATIONS }} formFactor="wide" />
      </div>
      <span className="text-xs text-muted-foreground">notification-center.json · the bell's popup</span>
      <div style={{ width: 380 }}>
        <ExponentialUiTree id="ede-notification-center" node={CENTER.root} data={{ notifications: SAMPLE_NOTIFICATIONS }} formFactor="wide" />
      </div>
      <span className="text-xs text-muted-foreground">tray-menu.json · a tray item's dbusmenu as the tray's popup</span>
      <div style={{ width: 280 }}>
        <ExponentialUiTree id="ede-tray-menu" node={TRAY_MENU.root} data={{ traymenu: SAMPLE_TRAY_MENU }} formFactor="wide" />
      </div>
      <span className="text-xs text-muted-foreground">settings.json · the settings surface, a page per domain over ede-services</span>
      <div style={{ width: 960 }}>
        <ExponentialUiTree id="ede-settings" node={SETTINGS.root} data={{ ...SETTINGS.data, services: SAMPLE_SERVICES, outputs: SAMPLE_OUTPUTS, user: { name: `Danny`, login: `danny` } }} formFactor="wide" />
      </div>
    </div>
  ),
}
