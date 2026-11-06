import { ExponentialUiTree } from "@exp/ui"
import type { NestedNode } from "@exponential_app/eui"

import ground from "../../../ede/ui/ground.json"
import sidebar from "../../../ede/ui/sidebar.json"
import taskbar from "../../../ede/ui/taskbar.json"
import startMenu from "../../../ede/ui/start-menu.json"
import chat from "../../../ede/ui/chat.json"
import expo from "../../../ede/ui/expo.json"
import scale from "../../../ede/ui/scale.json"
import history from "../../../ede/ui/history.json"
import switcher from "../../../ede/ui/switcher.json"
import osd from "../../../ede/ui/osd.json"
import notifications from "../../../ede/ui/notifications.json"
import center from "../../../ede/ui/notification-center.json"
import trayMenu from "../../../ede/ui/tray-menu.json"
import settings from "../../../ede/ui/settings.json"
import appletSound from "../../../ede/ui/applet-sound.json"
import appletNetwork from "../../../ede/ui/applet-network.json"
import appletBluetooth from "../../../ede/ui/applet-bluetooth.json"
import appletBattery from "../../../ede/ui/applet-battery.json"
import appletCalendar from "../../../ede/ui/applet-calendar.json"
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
const EXPO = expo as unknown as { root: NestedNode }
const SCALE = scale as unknown as { root: NestedNode }
const HISTORY = history as unknown as { root: NestedNode }
const SWITCHER = switcher as unknown as { root: NestedNode }
const OSD = osd as unknown as { root: NestedNode }
const NOTIFICATIONS = notifications as unknown as { root: NestedNode }
const CENTER = center as unknown as { root: NestedNode }
const TRAY_MENU = trayMenu as unknown as { root: NestedNode }
const SETTINGS = settings as unknown as { root: NestedNode; data: Record<string, unknown> }
const APPLET_SOUND = appletSound as unknown as { root: NestedNode }
const APPLET_NETWORK = appletNetwork as unknown as { root: NestedNode }
const APPLET_BLUETOOTH = appletBluetooth as unknown as { root: NestedNode }
const APPLET_BATTERY = appletBattery as unknown as { root: NestedNode }
const APPLET_CALENDAR = appletCalendar as unknown as { root: NestedNode }

const SAMPLE_SERVICES = {
  network: { available: true, wifiEnabled: true, devices: [{ path: `/dev/1`, interface: `wlp3s0`, state: `connected`, connection: `Home` }], accessPoints: [{ ssid: `Home`, strength: 82, active: true }, { ssid: `Cafe Nero`, strength: 47, active: false }] },
  sound: { available: true, volume: 40, muted: false, micVolume: 100, sinks: [{ id: 55, name: `Built-in Audio Analog Stereo`, volume: 40, default: true }] },
  bluetooth: { available: true, powered: true, devices: [{ path: `/bt/1`, name: `Keyboard K380`, address: `F4:73:35:12:AB:01`, connected: true }] },
  power: { available: true, batteryText: `73% · discharging`, profile: `balanced` },
  updates: { available: true, countText: `2`, packages: [{ name: `firefox`, from: `129.0`, version: `130.0` }] },
  printers: { available: true, printers: [{ name: `HP_LaserJet`, state: `idle`, default: true }] },
  about: { available: true, hostname: `ubuntu-wayland`, os: `Ubuntu 26.04 LTS`, kernel: `7.0.0-38-generic`, cpu: `13th Gen Intel Core i9-13900K`, memoryText: `7.2 GB`, ede: `0.1.0` },
}
// #EDE-43: the applets' sample services, what settings::demo_services feeds the goldens.
const SPOTIFY = { name: `org.mpris.MediaPlayer2.spotify`, identity: `Spotify`, status: `playing`, title: `Weightless`, artist: `Marconi Union`, album: `Weightless (Ambient Transmissions Vol. 2)`, artUrl: ``, position: 83, length: 485, positionText: `1:23 / 8:05`, canGoNext: true, canGoPrevious: true, canSeek: true }
const APPLET_SERVICES = {
  sound: { available: true, volume: 40, muted: false, sinks: [{ id: 55, name: `Built-in Audio Analog Stereo`, volume: 40, muted: false, default: true }, { id: 57, name: `HDMI Out`, volume: 100, muted: false, default: false }], sinkOptions: [{ label: `Built-in Audio Analog Stereo`, value: `55` }, { label: `HDMI Out`, value: `57` }], defaultSink: `55`, streams: [{ id: 71, app: `Firefox`, name: `Kurzgesagt – In a Nutshell`, volume: 80, muted: false }, { id: 74, app: `Spotify`, name: `Spotify`, volume: 65, muted: false }] },
  media: { available: true, players: [SPOTIFY], active: SPOTIFY },
  network: { available: true, wifiEnabled: true, wiredConnected: false, wired: [{ path: `/dev/2`, interface: `enp0s31f6`, state: `disconnected`, connection: `` }], accessPoints: [{ ssid: `Home`, strength: 82, secured: true, active: true }, { ssid: `Cafe Nero`, strength: 47, secured: false, active: false }, { ssid: `FRITZ!Box 7590`, strength: 31, secured: true, active: false }], vpn: [{ id: `6f2b1c0e-office`, name: `Office`, type: `vpn`, active: false }, { id: `9a4d7e21-home`, name: `Home WG`, type: `wireguard`, active: true }] },
  bluetooth: { available: true, powered: true, discovering: false, devices: [{ path: `/bt/1`, name: `Keyboard K380`, address: `F4:73:35:12:AB:01`, paired: true, connected: true }, { path: `/bt/2`, name: `WH-1000XM4`, address: `38:18:4C:00:11:22`, paired: true, connected: false }] },
  power: { available: true, battery: { present: true, percent: 73, state: `discharging`, timeText: `2:10 left` }, batteryText: `73% · 2:10 left`, profile: `balanced` },
}
const SAMPLE_DATE = { date: `2026-11-06`, dateText: { weekday: `Friday`, date: `6 November 2026`, long: `Friday, 6 November 2026` } }

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
const SAMPLE_EXPO = { workspaces: [
  { index: 0, label: `Workspace 1`, active: true, count: 3, countText: `3`, windows: [{ id: `w1`, title: `Firefox`, focused: false, thumbnail: `` }, { id: `w2`, title: `Terminal`, focused: true, thumbnail: `` }, { id: `w3`, title: `Files`, focused: false, thumbnail: `` }] },
  { index: 1, label: `Workspace 2`, active: false, count: 1, countText: `1`, windows: [{ id: `w4`, title: `Exponential — Firefox`, focused: false, thumbnail: `` }] },
] }
const SAMPLE_SCALE = { label: `Workspace 1`, windows: SAMPLE_EXPO.workspaces[0].windows }
const SAMPLE_HISTORY = { entries: [
  { id: `a1b2c3d4e5`, short: `a1b2c3d`, summary: `Edit taskbar.json`, at: `2026-11-06 11:58` },
  { id: `f6a7b8c9d0`, short: `f6a7b8c`, summary: `Edit sidebar.json`, at: `2026-11-06 11:40` },
  { id: `0f1e2d3c4b`, short: `0f1e2d3`, summary: `Factory sidebar.json`, at: `2026-11-06 09:00` },
] }
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
  blurb: `The desktop environment's factory shell documents, the IDE look (#EDE-42): the ground (apps/ede/ui/ground.json: the IDE's gradient and the rounded panel the focused window sits on), the sidebar (sidebar.json: the desktop/team switcher, inbox, search, new, the nav Needs you + Chats, the Pinned / Workspaces / Active sections, the Exponential start button and the chat button bottom-left; 272 px, or the 56 px icon rail), Mint's taskbar (taskbar.json: icon-only grouped buttons with pins, hover previews and a context menu, the applets, the bell and the clock at the right), the start menu (start-menu.json: the person and the favourites with lock / log out / power, the categories, the app rows, the search box), the chat (chat.json: the Assistant's composer), Expo and Scale (expo.json, scale.json: the workspaces and the active workspace's windows as live thumbnails), the version history (history.json: the shell documents' edits with Undo and Reset to factory), the Alt+Tab switcher (switcher.json, an on-demand overlay with live thumbnails), the OSD (osd.json, the media keys' level), the toasts of the shell's own notification server (notifications.json), the bell's notification centre (notification-center.json), a tray item's dbusmenu (tray-menu.json), the settings surface (settings.json, a page per domain over exponential-services) and the taskbar's applets (#EDE-43: applet-sound.json with the MPRIS card, the output volume and device and the per-app streams; applet-network.json with the Wired / Wi-Fi / VPN switches and the networks in reach; applet-bluetooth.json; applet-battery.json with the power mode; applet-calendar.json with the agenda and the inline month). Agents and people edit these files; exponential-ui validates, commits and rolls back (packages/ede). eDE-specific, not genUI: no new component, the core catalog only.`,
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
      <span className="text-xs text-muted-foreground">expo.json · Ctrl+Alt+Up, the workspaces with their windows</span>
      <div style={{ width: 1040 }}>
        <ExponentialUiTree id="ede-expo" node={EXPO.root} data={{ expo: SAMPLE_EXPO }} formFactor="wide" />
      </div>
      <span className="text-xs text-muted-foreground">scale.json · Ctrl+Alt+Down, the active workspace's windows</span>
      <div style={{ width: 1040 }}>
        <ExponentialUiTree id="ede-scale" node={SCALE.root} data={{ scale: SAMPLE_SCALE }} formFactor="wide" />
      </div>
      <span className="text-xs text-muted-foreground">history.json · the shell documents' edits, with Undo and Reset</span>
      <div style={{ width: 720 }}>
        <ExponentialUiTree id="ede-history" node={HISTORY.root} data={{ history: SAMPLE_HISTORY }} formFactor="wide" />
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
      <span className="text-xs text-muted-foreground">applet-sound.json · the sound button's popup: the player, the output, the applications</span>
      <div style={{ width: 380 }}>
        <ExponentialUiTree id="ede-applet-sound" node={APPLET_SOUND.root} data={{ services: APPLET_SERVICES, applets: { sound: { icon: `media-volume-up` } } }} formFactor="wide" />
      </div>
      <span className="text-xs text-muted-foreground">applet-network.json · the network button's popup</span>
      <div style={{ width: 380 }}>
        <ExponentialUiTree id="ede-applet-network" node={APPLET_NETWORK.root} data={{ services: APPLET_SERVICES }} formFactor="wide" />
      </div>
      <span className="text-xs text-muted-foreground">applet-bluetooth.json · the Bluetooth button's popup</span>
      <div style={{ width: 380 }}>
        <ExponentialUiTree id="ede-applet-bluetooth" node={APPLET_BLUETOOTH.root} data={{ services: APPLET_SERVICES }} formFactor="wide" />
      </div>
      <span className="text-xs text-muted-foreground">applet-battery.json · the battery button's popup</span>
      <div style={{ width: 380 }}>
        <ExponentialUiTree id="ede-applet-battery" node={APPLET_BATTERY.root} data={{ services: APPLET_SERVICES, applets: { battery: { show: true, icon: `ede-battery` } } }} formFactor="wide" />
      </div>
      <span className="text-xs text-muted-foreground">applet-calendar.json · the clock's popup: the agenda and the month</span>
      <div style={{ width: 720 }}>
        <ExponentialUiTree id="ede-applet-calendar" node={APPLET_CALENDAR.root} data={SAMPLE_DATE} formFactor="wide" />
      </div>
    </div>
  ),
}
