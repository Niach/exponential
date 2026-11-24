// #NEW-24 — what a view takes from the HOST's context (MCP Apps
// `hostContext`, on `ui/initialize` and again on every
// `ui/notifications/host-context-changed`, which carries only what changed):
//   - `theme`: light or dark. Toggles `html.dark`; a host that names none
//     leaves the view dark, as the app is everywhere.
//   - `styles.variables`: the host's style variables (`--color-background-
//     primary`, `--font-sans`, `--border-radius-md`…), set on the root so the
//     stylesheet's `var(--color-…, <ours>)` fallbacks resolve to the host's
//     and the card blends; `styles.css.fonts`: the faces those name.
//   - `safeAreaInsets`: px the host's chrome covers, as `--exp-safe-*` (the
//     view's root pads by them, styles.css).

import type { ColorMode } from "@exp/ui"
import type { HostContext } from "./bridge"

/** The spec's style variable families; nothing else is set on the root. */
const HOST_VARIABLE = /^--(?:color|font|border|shadow)-[a-z0-9-]+$/
const FONTS_STYLE_ID = `exp-host-fonts`
const SIDES = [`top`, `right`, `bottom`, `left`] as const

/** The mode a context names (null = it names none: keep the current one). */
export function hostColorMode(context: HostContext): ColorMode | null {
  return context.theme === `light` || context.theme === `dark` ? context.theme : null
}

export function applyColorMode(mode: ColorMode, doc: Document = document): void {
  doc.documentElement.classList.toggle(`dark`, mode === `dark`)
}

/** Applies what `context` names; returns the mode when it names one. */
export function applyHostContext(context: HostContext, doc: Document = document): ColorMode | null {
  const root = doc.documentElement
  const mode = hostColorMode(context)
  if (mode) applyColorMode(mode, doc)
  const variables = context.styles?.variables
  if (variables && typeof variables === `object`) {
    for (const [name, value] of Object.entries(variables)) {
      if (HOST_VARIABLE.test(name) && typeof value === `string` && value) root.style.setProperty(name, value)
    }
  }
  const fonts = context.styles?.css?.fonts
  if (typeof fonts === `string` && fonts && !doc.getElementById(FONTS_STYLE_ID)) {
    const style = doc.createElement(`style`)
    style.id = FONTS_STYLE_ID
    style.textContent = fonts
    doc.head.appendChild(style)
  }
  const insets = context.safeAreaInsets
  if (insets && typeof insets === `object`) {
    for (const side of SIDES) {
      const px = insets[side]
      root.style.setProperty(`--exp-safe-${side}`, `${typeof px === `number` && px > 0 ? px : 0}px`)
    }
  }
  return mode
}
