// Public TypeScript surface of the embeddable widget. The web app imports
// these types for its dogfood mount; embedders can copy them from the docs.

export interface ExponentialWidgetInitOptions {
  key: string
  // Override the API/bundle origin. Defaults to the origin the loader script
  // was served from.
  host?: string
  position?: `bottom-right` | `bottom-left`
  // Accent color for the floating button / primary actions (#rrggbb).
  color?: string
  // Button text; empty string renders an icon-only button.
  label?: string
  // false = no floating button; the host app calls open() itself.
  showButton?: boolean
  zIndex?: number
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

export interface ExponentialWidgetApi {
  init(options: ExponentialWidgetInitOptions): void
  identify(identity: ExponentialWidgetIdentity): void
  setCustomData(data: ExponentialWidgetCustomData): void
  open(): void
  close(): void
}

export type QueuedCall = [method: string, args: unknown[]]

// The snippet stub: queues calls until the loader takes over.
export interface ExponentialWidgetStub extends ExponentialWidgetApi {
  q?: QueuedCall[]
}

export interface WidgetRemoteForm {
  buttonLabel: string | null
  accentColor: string | null
  position: `bottom-right` | `bottom-left`
  emailRequired: boolean
}

export interface WidgetRemoteConfig {
  enabled: boolean
  form?: WidgetRemoteForm
  limits?: { maxScreenshotBytes: number }
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
}

declare global {
  interface Window {
    ExponentialWidget?: ExponentialWidgetStub
    __expWidget?: WidgetRuntimeState
  }
}
