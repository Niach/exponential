/* ─── DocShot — a real product screenshot from the committed store ───

   The docs used to embed the hand-built HTML mockups (src/webui, src/ide,
   src/mobile). Those stay: the home page and the Remotion movie still need a
   living, clickable app. But a docs page wants the PRODUCT, not a recreation
   of it, so every still embed here resolves through the view catalog
   (packages/view-catalog) and points at the store at /shots/<view>/<platform>.webp.

   Resolution is fail-fast on purpose: scripts/prerender.tsx renders every page
   under Bun, so a typo'd view id or a platform the catalog never declares
   throws during `bun run build` instead of shipping a 404'd <img>. The image
   FILE itself is not checked — shots are captured on a separate lane and the
   build must not depend on the store being populated. */

import {
  PLATFORM_FRAME,
  captureFor,
  viewById,
  type Platform,
} from "@exp/view-catalog"

export type DocShotProps = {
  /** A `views.json` view id, e.g. `board`, `issue-detail`, `support-inbox`. */
  view: string
  /** Defaults to the web app. Must be a platform the view declares. */
  platform?: Platform
  caption?: string
  /** Above the fold: skip lazy-loading so it paints with the page. */
  priority?: boolean
}

export function DocShot({
  view,
  platform = `web`,
  caption,
  priority = false,
}: DocShotProps) {
  const entry = viewById(view)
  if (!entry) {
    throw new Error(
      `DocShot: unknown view id "${view}" — it must be a view in packages/view-catalog/views.json`
    )
  }
  if (captureFor(entry, platform) === undefined) {
    throw new Error(
      `DocShot: view "${view}" declares no ${platform} capture — the store holds no ${platform} shot for it (see views.json${entry.notes?.[platform] ? `: ${entry.notes[platform]}` : ``})`
    )
  }

  const frame = PLATFORM_FRAME[platform]
  /* Portrait frames (phones, web-mobile) get the centered phone treatment the
     mockup embeds already use. */
  const portrait = frame.h > frame.w

  return (
    <figure
      className={`docs-embed docs-shot${portrait ? ` docs-embed-phone` : ``}`}
    >
      <img
        src={`/shots/${view}/${platform}.webp`}
        width={frame.w}
        height={frame.h}
        loading={priority ? `eager` : `lazy`}
        decoding="async"
        alt={entry.title}
      />
      {caption ? (
        <figcaption className="docs-embed-caption">{caption}</figcaption>
      ) : null}
    </figure>
  )
}
