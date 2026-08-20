// Public TypeScript surface of the embeddable widget. The web app imports
// these types for its dogfood mount; embedders can copy them from the docs.

// Panel + launcher theme (EXP-435). `auto` follows the visitor's
// prefers-color-scheme; absent = the widget config's theme, else dark.
export type ExponentialWidgetTheme = `dark` | `light` | `auto`

// Launcher appearance (EXP-569). `fab` is the floating pill (icon-only,
// hover-reveals its label); `tab` is a small edge-flush square with no label.
export type WidgetLauncherMode = `fab` | `tab`
export type WidgetLauncherPosition =
  | `top-left`
  | `top-right`
  | `middle-left`
  | `middle-right`
  | `bottom-left`
  | `bottom-right`
export interface WidgetLauncherPlacement {
  mode: WidgetLauncherMode
  position: WidgetLauncherPosition
}

export interface ExponentialWidgetInitOptions {
  key: string
  // Override the API/bundle origin. Defaults to the origin the loader script
  // was served from.
  host?: string
  // Legacy launcher position (pre-EXP-569): a bottom-corner FAB on both
  // devices. Ignored entirely when `launcher` is present.
  position?: `bottom-right` | `bottom-left`
  // Per-device launcher override (EXP-569). A field set here wins over the
  // widget config and the defaults (desktop: fab bottom-right, mobile: tab
  // middle-right); devices split at 767px viewport width.
  launcher?: {
    desktop?: Partial<WidgetLauncherPlacement>
    mobile?: Partial<WidgetLauncherPlacement>
  }
  // Accent color for the floating button / primary actions (#rrggbb).
  color?: string
  // Button text; empty string renders an icon-only button.
  label?: string
  // false = no floating button; the host app calls open() itself.
  showButton?: boolean
  zIndex?: number
  // Overrides the widget config's theme; setTheme() overrides both.
  theme?: ExponentialWidgetTheme
}

export interface ExponentialWidgetIdentity {
  email?: string
  name?: string
  userId?: string
}

export type ExponentialWidgetCustomData = Record<
  string,
  string | number | boolean
>

// Programmatic submission for hosts rolling their own UI (EXP-244 headless
// mode — pair with `showButton: false`). Identity and setCustomData state
// merge in exactly like a panel submission; the screenshot is host-supplied
// (headless never captures).
export interface ExponentialWidgetSubmitPayload {
  // Absent = `feedback`.
  mode?: WidgetMode
  // Feedback mode.
  title?: string
  description?: string
  screenshot?: Blob
  // Reporter-attached pictures (FEED-5) — up to 3 images, 10 MB each; extra
  // entries and non-Blobs are dropped.
  images?: Blob[]
  // Support mode.
  message?: string
  // Both modes; fall back to identify() values when absent.
  email?: string
  name?: string
  // Merged over identify-time custom data for this submission only.
  customData?: ExponentialWidgetCustomData
  // Feedback mode: ids of the widget's configured labels (EXP-435). Unknown
  // ids are dropped server-side.
  labels?: string[]
}

export interface ExponentialWidgetSubmitResult {
  ok: boolean
  // Feedback submissions carry the created issue identifier (e.g. "EXP-42").
  identifier?: string | null
  url?: string | null
  error?: string
  code?: string | null
}

export interface ExponentialWidgetApi {
  init(options: ExponentialWidgetInitOptions): void
  identify(identity: ExponentialWidgetIdentity): void
  setCustomData(data: ExponentialWidgetCustomData): void
  // Runtime theme override (EXP-435) — hook this to the host's own
  // dark/light toggle; launcher and panel update live. Wins over both the
  // init option and the widget config.
  setTheme(theme: ExponentialWidgetTheme): void
  open(): void
  close(): void
  // Resolves with an error result instead of throwing. NOTE: calls queued
  // through the snippet stub (before loader.js executes) run fire-and-forget
  // and return undefined — call from user interaction handlers, where the
  // loader is long loaded, to get the Promise.
  submit(
    payload: ExponentialWidgetSubmitPayload
  ): Promise<ExponentialWidgetSubmitResult>
}

export type QueuedCall = [method: string, args: unknown[]]

// The snippet stub: queues calls until the loader takes over.
export interface ExponentialWidgetStub extends ExponentialWidgetApi {
  q?: QueuedCall[]
}

// Owner-defined extra feedback-form input (EXP-244); the typed value lands
// in the submission's customData blob under `key`.
export interface WidgetCustomField {
  key: string
  label: string
  required?: boolean
}

// A team label the reporter may tag their submission with (EXP-435),
// resolved server-side from the widget config.
export interface WidgetRemoteLabel {
  id: string
  name: string
  color: string
}

export interface WidgetRemoteForm {
  buttonLabel: string | null
  accentColor: string | null
  // Legacy launcher position, still served for cached pre-EXP-569 bundles;
  // current clients read `launcher` and only fall back here against old
  // self-hosted servers.
  position: `bottom-right` | `bottom-left`
  emailRequired: boolean
  // EXP-244 field toggles — absent on older servers: collectEmail defaults
  // true, collectName false (legacy behavior).
  collectEmail?: boolean
  collectName?: boolean
  nameRequired?: boolean
  customFields?: WidgetCustomField[]
  // EXP-435 additions — absent on older servers: theme null/absent = dark,
  // no labels.
  theme?: ExponentialWidgetTheme | null
  labels?: WidgetRemoteLabel[]
  // EXP-569 — server-resolved per-device launcher placements (absent on
  // older servers). `iconSvg` is the owner-picked icon's markup, rendered
  // server-side from the shared icon registry; null/absent = the built-in
  // megaphone.
  launcher?: {
    desktop: WidgetLauncherPlacement
    mobile: WidgetLauncherPlacement
    iconSvg?: string | null
  }
}

// Which entry points the panel offers (EXP-130).
export type WidgetMode = `feedback` | `support`

export interface WidgetRemoteConfig {
  enabled: boolean
  // Absent on older servers = feedback-only.
  modes?: WidgetMode[]
  form?: WidgetRemoteForm
  // maxImageBytes/maxImages absent on pre-FEED-5 servers.
  limits?: {
    maxScreenshotBytes: number
    maxImageBytes?: number
    maxImages?: number
  }
}

// Hooks the main bundle registers on the shared runtime state so the
// loader-owned `window.ExponentialWidget` methods can delegate to it. The
// API object the snippet created never changes identity.
export interface WidgetBundleHooks {
  open(): void
  close(): void
  stateChanged(): void
}

// Shared state between loader and lazily-injected main bundle, hung off
// `window.__expWidget`. `protocol` guards loader/bundle cache-skew: a bundle
// seeing an unknown protocol number no-ops with a console warning instead of
// breaking the host page.
export interface WidgetRuntimeState {
  protocol: 1
  options: ExponentialWidgetInitOptions
  identity: ExponentialWidgetIdentity
  customData: ExponentialWidgetCustomData
  apiOrigin: string
  bundleUrl: string
  configPromise: Promise<WidgetRemoteConfig | null>
  config: WidgetRemoteConfig | null
  disabled: boolean
  openRequested: boolean
  bundleInjected: boolean
  loaderButtonHost: HTMLElement | null
  bundle: WidgetBundleHooks | null
  // setTheme() runtime override (EXP-435). Optional so pre-theme state
  // literals (tests, cached loaders) stay valid.
  themeOverride?: ExponentialWidgetTheme | null
}

declare global {
  interface Window {
    ExponentialWidget?: ExponentialWidgetStub
    __expWidget?: WidgetRuntimeState
  }
}
