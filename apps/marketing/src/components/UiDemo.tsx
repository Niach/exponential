/* ─── UiDemo — a real @exp/ui component inside a docs page (EXP-887) ───

   The docs show product SCREENSHOTS (DocShot) and hand-built recreations
   (src/ide, src/webui, src/mobile). This is the third kind: the actual
   component, shipped as static markup in a declarative shadow root by
   scripts/ui-demos.tsx, so a chip in the docs cannot drift from the chip in
   the app.

   EXP-903: the recreations stay DRAWINGS on purpose — a fake app at marketing's
   own scale and hex tokens, not `@exp/ui` — and an island replaces one only
   where a docs page shows the control ITSELF rather than a screen around it.

   The shadow root is the whole point — the web theme's tokens share names with
   marketing's own (`--border`, `--input`, `--accent`) and its preflight would
   reset the site, so the stylesheet rides inside the island and reaches
   nothing else. `.ui-demo` (styles/docs.css) paints the app's ground under it.

   Two paths, one markup blob:
   · prerender + hydrate — the parser attaches the shadow root from the
     `<template shadowrootmode="open">`, and React 19's hydration arm for
     `dangerouslySetInnerHTML` only COMPARES, so the tree survives untouched;
   · `vite dev`, where the page is client-rendered — `innerHTML` does not
     process declarative shadow DOM, so the effect attaches it by hand. The
     `shadowRoot` guard makes that a no-op after the first path and keeps it
     idempotent under StrictMode's double-invoked effects. */

import { useEffect, useRef } from "react"

import { UI_DEMOS } from "../ui-demos/generated"

export function UiDemo({ id }: { id: string }) {
  const hostRef = useRef<HTMLDivElement>(null)
  const html = UI_DEMOS[id]

  useEffect(() => {
    const island = hostRef.current?.querySelector(`[data-ui-island]`)
    if (!island || island.shadowRoot) return
    const template = island.querySelector(`:scope > template`)
    if (!(template instanceof HTMLTemplateElement)) return
    island
      .attachShadow({ mode: `open` })
      .appendChild(template.content.cloneNode(true))
    // `html` is a dep too: under `vite dev`, regenerating demos.tsx hands
    // React a new blob for the SAME id, `innerHTML` replaces the div that
    // owned the shadow root, and the guard above lets this re-attach.
  }, [id, html])

  if (!html) {
    /* Fail the BUILD, not the page: scripts/prerender.tsx renders every docs
       page under Bun, so a typo'd id throws there instead of shipping an
       empty frame. */
    throw new Error(
      `UiDemo: unknown demo id "${id}" — ids are the keys of src/ui-demos/demos.tsx (${Object.keys(UI_DEMOS).join(`, `)})`
    )
  }

  return (
    <div
      className="ui-demo"
      ref={hostRef}
      suppressHydrationWarning
      dangerouslySetInnerHTML={{ __html: html }}
    />
  )
}
