/* A stored platform shot (shots/<view>/<platform>.webp). The shot index
   (scripts/shot-index.ts) decides at prerender time whether the store has
   it: a missing shot renders the "shot pending" box, never a broken image
   (an image that still fails to load swaps to the same box). */
import { useEffect, useRef, useState } from "react"
import { SHOT_INDEX } from "../generated/shot-index"
import { shotSrc, type ShotPlatform as Platform } from "./platforms"

/** Natural sizes of the stored shots (phones portrait, web 3:2, desktop window). */
export const SHOT_SIZES: Record<string, { w: number; h: number }> = {
  web: { w: 1800, h: 1200 },
  desktop: { w: 1800, h: 1126 },
  ios: { w: 828, h: 1800 },
  android: { w: 810, h: 1800 },
}

export const shotExists = (viewId: string, platform: string) => SHOT_INDEX[viewId]?.includes(platform) ?? false

export function ShotPending({ platform, className }: { platform: string; className?: string }) {
  const size = SHOT_SIZES[platform] ?? SHOT_SIZES.web!
  return (
    <div className={`sdk-shot-missing ${className ?? ``}`} style={{ aspectRatio: `${size.w} / ${size.h}` }} role="img" aria-label={`${platform} shot pending`}>
      <span>Shot pending</span>
    </div>
  )
}

export function ShotImage({ viewId, platform, alt, className, eager }: { viewId: string; platform: string; alt: string; className?: string; eager?: boolean }) {
  const ref = useRef<HTMLImageElement>(null)
  const [failed, setFailed] = useState(false)
  useEffect(() => {
    const img = ref.current
    if (img && img.complete && img.naturalWidth === 0) setFailed(true)
  }, [])
  const size = SHOT_SIZES[platform] ?? SHOT_SIZES.web!
  if (failed || !shotExists(viewId, platform)) return <ShotPending platform={platform} className={className} />
  return <img ref={ref} className={className} src={shotSrc(viewId, platform)} alt={alt} width={size.w} height={size.h} loading={eager ? `eager` : `lazy`} decoding="async" onError={() => setFailed(true)} />
}

export function ShotFigure({ viewId, platform, title, compact }: { viewId: string; platform: Platform; title: string; compact?: boolean }) {
  const phone = platform.id === `ios` || platform.id === `android`
  return (
    <figure className={`sdk-shot is-${platform.id}${phone ? ` is-phone` : ``}${compact ? ` is-compact` : ``}`}>
      <div className="sdk-shot-frame">
        <ShotImage viewId={viewId} platform={platform.id} alt={`${title} rendered by the ${platform.renderer} renderer on ${platform.label}`} />
      </div>
      <figcaption>
        <strong>{platform.label}</strong> <span>{platform.renderer}</span>
      </figcaption>
    </figure>
  )
}
