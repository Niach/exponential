/* ─── Blog figures ───
   `Figure` = one picture with a caption; `concept` stamps a drafted image
   (never a capture) so a reader can tell the two apart. `Shot` resolves a
   real capture from the committed store through the view catalog (the docs'
   DocShot rule: an unknown view or platform throws during the prerender).
   `ShotRow` sets two phone shots side by side. */
import type { ReactNode } from "react"
import type { Platform } from "@exp/view-catalog"
import { shotSrc } from "../DocShot"

export type FigureProps = {
  src: string
  alt: string
  width: number
  height: number
  caption?: ReactNode
  /** A drafted image, not a capture: shows the "concept" stamp. */
  concept?: boolean
  /** Portrait frame (a phone): centered, narrower. */
  phone?: boolean
  /** Above the fold: eager loading. */
  priority?: boolean
}

export function Figure({ src, alt, width, height, caption, concept = false, phone = false, priority = false }: FigureProps) {
  return (
    <figure className={`docs-shot blog-figure${phone ? ` docs-embed-phone` : ``}`}>
      <div className="blog-figure-frame">
        <img src={src} alt={alt} width={width} height={height} loading={priority ? `eager` : `lazy`} decoding="async" />
        {concept ? <span className="blog-concept">concept</span> : null}
      </div>
      {caption ? <figcaption className="docs-embed-caption">{caption}</figcaption> : null}
    </figure>
  )
}

export type ShotProps = {
  /** A `views.json` view id. */
  view: string
  platform?: Platform
  caption?: ReactNode
  priority?: boolean
}

/** A real capture from shots/<view>/<platform>.webp. */
export function Shot({ view, platform = `web`, caption, priority = false }: ShotProps) {
  const { src, frame, title } = shotSrc(view, platform)
  return <Figure src={src} alt={`${title} on ${platform}`} width={frame.w} height={frame.h} caption={caption} phone={frame.h > frame.w} priority={priority} />
}

export function ShotRow({ children }: { children: ReactNode }) {
  return <div className="docs-shot-row">{children}</div>
}
