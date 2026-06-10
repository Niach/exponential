// The snippet target (<2KB min goal): replaces the snippet's queue stub with
// a live API, renders the floating button WITHOUT loading Preact/snapdom,
// prefetches the remote config, and injects widget.js on first open. Must
// never throw into the host page.
import type {
  ExponentialWidgetInitOptions,
  ExponentialWidgetStub,
  QueuedCall,
  WidgetRemoteConfig,
  WidgetRuntimeState,
} from "./types"
import {
  buttonCss,
  defaultZIndex,
  megaphoneIconSvg,
  theme,
} from "./theme"

function currentScriptSrc(): string | null {
  const current = document.currentScript
  if (current instanceof HTMLScriptElement && current.src) return current.src
  const fallback = document.querySelector<HTMLScriptElement>(
    `script[src*="/widget/v1/loader.js"]`
  )
  return fallback?.src ?? null
}

function warn(message: string): void {
  console.warn(`[ExponentialWidget] ${message}`)
}

function start(): void {
  if (window.__expWidget) return

  const src = currentScriptSrc()
  if (!src) {
    warn(`could not determine loader script origin`)
    return
  }

  const previous = window.ExponentialWidget
  const pendingQueue: QueuedCall[] = previous?.q ? [...previous.q] : []

  let state: WidgetRuntimeState | null = null

  function injectBundle(): void {
    if (!state || state.bundleInjected || state.disabled) return
    state.bundleInjected = true
    const script = document.createElement(`script`)
    script.async = true
    script.src = state.bundleUrl
    script.onerror = () => {
      // Clean up + allow a retry on the next open().
      script.remove()
      if (state) state.bundleInjected = false
      warn(`failed to load widget bundle`)
    }
    document.head.appendChild(script)
  }

  function renderButton(): void {
    if (!state || state.options.showButton === false) return
    if (state.loaderButtonHost || state.disabled) return

    const host = document.createElement(`div`)
    host.setAttribute(`data-exponential-widget`, ``)
    host.style.cssText = `all:initial;position:fixed;bottom:20px;${
      resolvedPosition() === `bottom-left` ? `left:20px` : `right:20px`
    };z-index:${state.options.zIndex ?? defaultZIndex};`

    const root = host.attachShadow({ mode: `open` })
    const style = document.createElement(`style`)
    style.textContent = buttonCss(resolvedAccent())
    const button = document.createElement(`button`)
    button.className = `exp-fab`
    button.setAttribute(`aria-label`, `Send feedback`)
    button.setAttribute(`aria-haspopup`, `dialog`)
    button.innerHTML = `${megaphoneIconSvg}${
      resolvedLabel() ? `<span></span>` : ``
    }`
    const labelSpan = button.querySelector(`span`)
    if (labelSpan) labelSpan.textContent = resolvedLabel()
    button.addEventListener(`click`, () => api.open())
    root.append(style, button)
    document.body.appendChild(host)
    state.loaderButtonHost = host
  }

  function resolvedAccent(): string {
    return (
      state?.options.color ??
      state?.config?.form?.accentColor ??
      theme.defaultAccent
    )
  }

  function resolvedLabel(): string {
    const label =
      state?.options.label ?? state?.config?.form?.buttonLabel ?? `Feedback`
    return label
  }

  function resolvedPosition(): `bottom-right` | `bottom-left` {
    return (
      state?.options.position ?? state?.config?.form?.position ?? `bottom-right`
    )
  }

  function restyleButton(): void {
    if (!state?.loaderButtonHost) return
    state.loaderButtonHost.remove()
    state.loaderButtonHost = null
    renderButton()
  }

  function whenBodyReady(fn: () => void): void {
    if (document.body) {
      fn()
      return
    }
    document.addEventListener(`DOMContentLoaded`, fn, { once: true })
  }

  const api: ExponentialWidgetStub = {
    init(options: ExponentialWidgetInitOptions) {
      if (state) {
        warn(`already initialized`)
        return
      }
      if (!options?.key) {
        console.error(`[ExponentialWidget] init requires a key`)
        return
      }

      const apiOrigin = (options.host ?? new URL(src).origin).replace(
        /\/$/,
        ``
      )
      const bundleUrl = src.replace(/loader\.js(\?.*)?$/, `widget.js`)

      const runtime: WidgetRuntimeState = {
        protocol: 1,
        options,
        identity: {},
        customData: {},
        apiOrigin,
        bundleUrl,
        config: null,
        disabled: false,
        openRequested: false,
        bundleInjected: false,
        loaderButtonHost: null,
        bundle: null,
        configPromise: fetch(
          `${apiOrigin}/api/widget/config?key=${encodeURIComponent(options.key)}`,
          { credentials: `omit` }
        )
          .then((response) =>
            response.ok ? (response.json() as Promise<WidgetRemoteConfig>) : null
          )
          .catch(() => null),
      }
      state = runtime
      window.__expWidget = runtime

      void runtime.configPromise.then((config) => {
        runtime.config = config
        if (config && !config.enabled) {
          runtime.disabled = true
          runtime.loaderButtonHost?.remove()
          runtime.loaderButtonHost = null
          return
        }
        restyleButton()
      })

      whenBodyReady(renderButton)
    },

    identify(identity) {
      if (!state) return
      state.identity = { ...state.identity, ...identity }
      state.bundle?.stateChanged()
    },

    setCustomData(data) {
      if (!state) return
      state.customData = { ...state.customData, ...data }
      state.bundle?.stateChanged()
    },

    open() {
      if (!state || state.disabled) return
      if (state.bundle) {
        state.bundle.open()
        return
      }
      state.openRequested = true
      injectBundle()
    },

    close() {
      if (!state) return
      state.openRequested = false
      state.bundle?.close()
    },
  }

  window.ExponentialWidget = api

  for (const [method, args] of pendingQueue) {
    const fn = api[method as keyof ExponentialWidgetStub]
    if (typeof fn === `function`) {
      try {
        ;(fn as (...callArgs: unknown[]) => void)(...args)
      } catch {
        warn(`queued ${method} call failed`)
      }
    }
  }
}

if (typeof document !== `undefined`) {
  try {
    start()
  } catch (error) {
    console.warn(`[ExponentialWidget] loader failed`, error)
  }
}

export {}
