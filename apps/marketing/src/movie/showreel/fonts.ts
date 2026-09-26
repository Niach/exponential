// showreel/fonts.ts — loads the marketing site's self-hosted faces
// (public/fonts, mirrored from fonts.css) through the FontFace API and holds
// the render until they are in: a Remotion render is headless Chrome with no
// stylesheet, and @font-face alone would race the first frames. The same
// files the site serves, so studio, render and the page agree on every glyph.

import { useEffect } from "react"
import { continueRender, delayRender, staticFile } from "remotion"

const FACES = [
  { family: `Geist`, file: `geist-latin.woff2`, weight: `300 700` },
  { family: `Geist Mono`, file: `geist-mono-latin.woff2`, weight: `400 600` },
  { family: `Inter`, file: `inter-latin.woff2`, weight: `400 700` },
  {
    family: `JetBrains Mono`,
    file: `jetbrains-mono-latin.woff2`,
    weight: `400 600`,
  },
] as const

export const useShowreelFonts = (): void => {
  useEffect(() => {
    const handle = delayRender(`showreel fonts`)
    Promise.all(
      FACES.map(async ({ family, file, weight }) => {
        const face = new FontFace(
          family,
          `url(${staticFile(`fonts/${file}`)}) format("woff2")`,
          { weight, style: `normal` }
        )
        await face.load()
        document.fonts.add(face)
      })
    )
      .catch(() => undefined)
      .finally(() => continueRender(handle))
  }, [])
}
