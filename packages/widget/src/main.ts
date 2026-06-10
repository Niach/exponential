// The lazily-injected main bundle: reads the shared runtime state the loader
// prepared, mounts the Preact app in a shadow root, and registers hooks so
// the loader-owned `window.ExponentialWidget` delegates here from now on.
import { h, render } from "preact"
import { App } from "./ui/App"
import { buttonCss, defaultZIndex, theme } from "./theme"
import widgetCss from "./ui/widget.css?inline"

function mount(): void {
  const state = window.__expWidget
  if (!state) {
    console.warn(`[ExponentialWidget] bundle loaded without loader state`)
    return
  }
  if (state.protocol !== 1) {
    // Loader/bundle cache skew: no-op rather than risk breaking the host.
    console.warn(`[ExponentialWidget] loader/bundle version mismatch`)
    return
  }
  if (state.bundle) return

  const host = document.createElement(`div`)
  host.setAttribute(`data-exponential-widget`, ``)
  host.style.cssText = `all:initial;position:fixed;top:0;left:0;width:0;height:0;z-index:${
    state.options.zIndex ?? defaultZIndex
  };`
  const root = host.attachShadow({ mode: `open` })

  const style = document.createElement(`style`)
  const accent =
    state.options.color ?? state.config?.form?.accentColor ?? theme.defaultAccent
  style.textContent = `${widgetCss}\n${buttonCss(accent)}`
  root.appendChild(style)

  const container = document.createElement(`div`)
  root.appendChild(container)
  document.body.appendChild(host)

  // The Preact app renders its own (identical) button; drop the loader's.
  state.loaderButtonHost?.remove()
  state.loaderButtonHost = null

  render(h(App, { state }), container)
}

if (typeof document !== `undefined`) {
  try {
    if (document.body) {
      mount()
    } else {
      document.addEventListener(`DOMContentLoaded`, mount, { once: true })
    }
  } catch (error) {
    console.warn(`[ExponentialWidget] failed to mount`, error)
  }
}

export {}
