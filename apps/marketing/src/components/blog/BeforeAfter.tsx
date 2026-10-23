/* Two pictures of the same size under one slider: drag to reveal `before`
   over `after`. The initial position is the same on the prerender and the
   client (no hydration mismatch); the range input is the control, so it
   works with a keyboard and a screen reader. */
import { useState } from "react"

export type Pane = { src: string; alt: string; label: string }

export function BeforeAfter({ before, after, width, height, caption }: { before: Pane; after: Pane; width: number; height: number; caption?: string }) {
  const [pos, setPos] = useState(50)
  return (
    <figure className="docs-shot blog-figure blog-compare">
      <div className="blog-figure-frame blog-compare-frame" style={{ aspectRatio: `${width} / ${height}` }}>
        <img className="blog-compare-img" src={after.src} alt={after.alt} width={width} height={height} loading="lazy" decoding="async" />
        <div className="blog-compare-before" style={{ clipPath: `inset(0 ${100 - pos}% 0 0)` }}>
          <img className="blog-compare-img" src={before.src} alt={before.alt} width={width} height={height} loading="lazy" decoding="async" />
        </div>
        <span className="blog-compare-handle" style={{ left: `${pos}%` }} aria-hidden />
        <span className="blog-compare-label is-before">{before.label}</span>
        <span className="blog-compare-label is-after">{after.label}</span>
        <input
          className="blog-compare-range"
          type="range"
          min={0}
          max={100}
          value={pos}
          onChange={(e) => setPos(Number(e.currentTarget.value))}
          aria-label={`Reveal ${before.label} over ${after.label}`}
        />
      </div>
      {caption ? <figcaption className="docs-embed-caption">{caption}</figcaption> : null}
    </figure>
  )
}
