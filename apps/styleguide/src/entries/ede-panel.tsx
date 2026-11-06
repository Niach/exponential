import { ExponentialUiTree } from "@exp/ui"
import type { NestedNode } from "@exponential_app/eui"

import ground from "../../../ede/ui/ground.json"
import sidebar from "../../../ede/ui/sidebar.json"
import taskbar from "../../../ede/ui/taskbar.json"
import startMenu from "../../../ede/ui/start-menu.json"
import chat from "../../../ede/ui/chat.json"
import switcher from "../../../ede/ui/switcher.json"
import osd from "../../../ede/ui/osd.json"
import notifications from "../../../ede/ui/notifications.json"
import center from "../../../ede/ui/notification-center.json"
import trayMenu from "../../../ede/ui/tray-menu.json"
import settings from "../../../ede/ui/settings.json"
import type { StyleguideEntry } from "./types.ts"

// #EDE-10 / #EDE-11 / #EDE-12 / #EDE-42: eDE's factory shell documents (apps/ede/ui),
// the genUI trees exponential-shell draws as layer-shell surfaces on exponential-compositor:
// the IDE look (the ground, the sidebar with its rail, Mint's taskbar, the start
// menu, the chat) and the overlays. The same JSON, the web renderer here, the
// gpui painter on desktop and in eDE itself (shots/ede-* are the compositor's
// own captures). The `surface` header (where the shell puts it, which host
// functions it may call) is not part of the tree.

const GROUND = ground as unknown as { root: NestedNode }
const SIDEBAR = sidebar as unknown as { root: NestedNode }
const TASKBAR = taskbar as unknown as { root: NestedNode; data: Record<string, unknown> }
const START_MENU = startMenu as unknown as { root: NestedNode }
const CHAT = chat as unknown as { root: NestedNode }
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
const SAMPLE_OUTPUTS = [{ name: `HEADLESS-1`, summary: `Exponential Desktop (headless) · 1280×720 at 1× (0, 0)`, current: `1280x720@60`, modeOptions: [{ label: `1280x720@60 (preferred)`, value: `1280x720@60` }], scale: 1, scaleText: `1` }]

const SAMPLE_NOTIFICATIONS = {
  toasts: [{ id: 1, app: `Thunderbird`, summary: `Release notes for 1.4`, body: `Danny: the M1 demo runs on Friday, bring the VM.`, icon: ``, actions: [{ key: `open`, label: `Open` }, { key: `archive`, label: `Archive` }], urgency: 1, at: `11:58` }],
  list: [
    { id: 1, app: `Thunderbird`, summary: `Release notes for 1.4`, body: `Danny: the M1 demo runs on Friday, bring the VM.`, icon: ``, actions: [], urgency: 1, at: `11:58` },
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
  { id: `org.gnome.Calculator`, name: `Calculator`, comment: `Perform arithmetic, scientific or financial calculations`, exec: `gnome-calculator`, icon: `` },
  { id: `at.exponential`, name: `Exponential`, comment: `Your team, agents and boards`, exec: `exp-desktop`, icon: `` },
  { id: `nemo`, name: `Files`, comment: `Access and organize files`, exec: `nemo`, icon: `` },
  { id: `firefox`, name: `Firefox`, comment: `Browse the web`, exec: `firefox`, icon: `` },
  { id: `terminal`, name: `Terminal`, comment: `Use the command line`, exec: `x-terminal-emulator`, icon: `` },
]
const SAMPLE_SHELL = {
  rail: false,
  switcher: { current: { kind: `local`, id: `local`, name: `Local` }, entries: [{ kind: `local`, id: `local`, name: `Local`, current: true }] },
  sidebar: { pinnedOpen: true, workspacesOpen: true, sessionsOpen: true, pinned: [{ app: `firefox`, name: `Firefox`, icon: ``, focused: false }, { app: `nemo`, name: `Files`, icon: ``, focused: false }], workspaces: [{ index: 0, label: `Workspace 1`, active: true, count: 3, countText: `3` }, { index: 1, label: `Workspace 2`, active: false, count: 1, countText: `1` }] },
  sessions: [],
  taskbar: { pinned: [`firefox`, `nemo`], hover: null, previews: null, menu: null, buttons: [
    { app: `firefox`, name: `Firefox`, icon: ``, pinned: true, windows: [`w1`, `w4`], focused: false, minimized: false, running: true },
    { app: `nemo`, name: `Files`, icon: ``, pinned: true, windows: [`w3`], focused: false, minimized: false, running: true },
    { app: `org.gnome.Calculator`, name: `Calculator`, icon: ``, pinned: true, windows: [], focused: false, minimized: false, running: false },
    { app: `terminal`, name: `Terminal`, icon: ``, pinned: false, windows: [`w2`], focused: true, minimized: false, running: true },
  ] },
  startMenu: { open: true, query: ``, category: `all`, categories: [
    { id: `all`, label: `All applications`, count: 5, icon: `nav-apps`, selected: true }, { id: `favourites`, label: `Favourites`, count: 2, icon: `ui-pin`, selected: false }, { id: `exponential`, label: `Exponential`, count: 1, icon: `brand-exponential`, selected: false }, { id: `accessories`, label: `Accessories`, count: 3, icon: `ui-properties`, selected: false }, { id: `internet`, label: `Internet`, count: 1, icon: `ui-link`, selected: false }, { id: `administration`, label: `Administration`, count: 1, icon: `nav-settings`, selected: false },
  ], apps: SAMPLE_APPS.map((a) => ({ ...a, category: `accessories` })), favourites: [{ id: `firefox`, name: `Firefox`, icon: `` }, { id: `nemo`, name: `Files`, icon: `` }] },
  chat: { open: false },
  altTab: { open: false, selected: 0, order: [] },
}
const SAMPLE_SWITCHER = { windows: [
  { id: `w1`, title: `Firefox`, selected: false, thumbnail: `` },
  { id: `w2`, title: `Terminal`, selected: true, thumbnail: `` },
  { id: `w3`, title: `Files`, selected: false, thumbnail: `` },
] }

export const entry: StyleguideEntry = {
  id: `ede-panel`,
  section: `special`,
  owner: `EDE-42`,
  title: `Exponential Desktop shell`,
  blurb: `The desktop environment's factory shell documents, the IDE look (#EDE-42): the ground (apps/ede/ui/ground.json: the IDE's gradient and the rounded panel the focused window sits on), the sidebar (sidebar.json: the desktop/team switcher, inbox, search, new, the nav Needs you + Chats, the Pinned / Workspaces / Active sections, the Exponential start button and the chat button bottom-left; 272 px, or the 56 px icon rail), Mint's taskbar (taskbar.json: icon-only grouped buttons with pins, hover previews and a context menu, the applets, the bell and the clock at the right), the start menu (start-menu.json: the person and the favourites with lock / log out / power, the categories, the app rows, the search box), the chat (chat.json: the Assistant's composer), the Alt+Tab switcher (switcher.json, an on-demand overlay with live thumbnails), the OSD (osd.json, the media keys' level), the toasts of the shell's own notification server (notifications.json), the bell's notification centre (notification-center.json), a tray item's dbusmenu (tray-menu.json) and the settings surface (settings.json, a page per domain over exponential-services). Agents and people edit these files; exponential-ui validates, commits and rolls back (packages/ede). eDE-specific, not genUI: no new component, the core catalog only.`,
  status: {
    web: {
      state: `ok`,
      symbol: `ExponentialUiTree`,
      file: `packages/ui/src/exponential-ui-specimen.tsx`,
      note: `here only: the React renderer over apps/ede/ui/*.json (the app has no desktop shell)`,
    },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/ede_panel.rs`, note: `the styleguide twin; the real thing is exponential-shell (apps/desktop/crates/ede-shell) on exponential-compositor` },
    ios: { state: `n/a`, note: `Exponential Desktop only: a desktop environment` },
    android: { state: `n/a`, note: `Exponential Desktop only: a desktop environment` },
  },
  island: () => (
    <div className="flex flex-col gap-3">
      <span className="text-xs text-muted-foreground">ground.json · the background surface: the IDE's gradient and the panel</span>
      <div style={{ width: 640, height: 360 }}>
        <ExponentialUiTree id="ede-ground" node={GROUND.root} data={{ shell: SAMPLE_SHELL }} formFactor="wide" />
      </div>
      <span className="text-xs text-muted-foreground">sidebar.json · the left column, 272 px (the rail: 56 px)</span>
      <div style={{ display: `flex`, gap: 12 }}>
        <div style={{ width: 272, height: 560 }}>
          <ExponentialUiTree id="ede-sidebar" node={SIDEBAR.root} data={{ shell: SAMPLE_SHELL }} formFactor="wide" />
        </div>
        <div style={{ width: 56, height: 560 }}>
          <ExponentialUiTree id="ede-rail" node={SIDEBAR.root} data={{ shell: { ...SAMPLE_SHELL, rail: true } }} formFactor="wide" />
        </div>
      </div>
      <span className="text-xs text-muted-foreground">taskbar.json · Mint's grouped window list, 48 px along the bottom</span>
      <div style={{ width: 1008 }}>
        <ExponentialUiTree id="ede-taskbar" node={TASKBAR.root} data={{ ...TASKBAR.data, time: `12:00`, shell: SAMPLE_SHELL, tray: [{ service: `demo/network`, title: `Network`, status: `Active`, icon: `` }], notifications: { count: 2 }, applets: { network: { icon: `ede-wifi` }, sound: { icon: `media-volume-up` }, bluetooth: { show: true }, battery: { show: true, icon: `ede-battery` } } }} formFactor="wide" />
      </div>
      <span className="text-xs text-muted-foreground">start-menu.json · the start button's popup surface, 760 × 620</span>
      <div style={{ width: 760 }}>
        <ExponentialUiTree id="ede-start-menu" node={START_MENU.root} data={{ user: { name: `Danny`, host: `ubuntu-wayland` }, shell: SAMPLE_SHELL }} formFactor="wide" />
      </div>
      <span className="text-xs text-muted-foreground">chat.json · the chat button's surface</span>
      <div style={{ width: 720 }}>
        <ExponentialUiTree id="ede-chat" node={CHAT.root} data={{ chat: { draft: `` } }} formFactor="wide" />
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
      <span className="text-xs text-muted-foreground">settings.json · the settings surface, a page per domain over exponential-services</span>
      <div style={{ width: 960 }}>
        <ExponentialUiTree id="ede-settings" node={SETTINGS.root} data={{ ...SETTINGS.data, services: SAMPLE_SERVICES, outputs: SAMPLE_OUTPUTS, user: { name: `Danny`, login: `danny` } }} formFactor="wide" />
      </div>
    </div>
  ),
}
