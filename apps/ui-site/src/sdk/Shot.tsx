/* A stored platform shot (shots/<view>/<platform>.webp) with a fallback
   when the store has none yet. The image is real prerendered markup; an
   error that fired before hydration is caught by checking the element. */
import { useEffect, useRef, useState } from "react"
import { shotSrc, type ShotPlatform as Platform } from "./platforms"

/** Natural sizes of the stored shots (phones portrait, web 3:2, desktop window). */
const SIZES: Record<string, { w: number; h: number }> = {
  web: { w: 1800, h: 1200 },
  desktop: { w: 1800, h: 1126 },
  ios: { w: 828, h: 1800 },
  android: { w: 810, h: 1800 },
}

export function ShotImage({ viewId, platform, alt, className, eager }: { viewId: string; platform: string; alt: string; className?: string; eager?: boolean }) {
  const ref = useRef<HTMLImageElement>(null)
  const [failed, setFailed] = useState(false)
  useEffect(() => {
    const img = ref.current
    if (img && img.complete && img.naturalWidth === 0) setFailed(true)
  }, [])
  const size = SIZES[platform] ?? SIZES.web!
  if (failed)
    return (
      <div className={`sdk-shot-missing ${className ?? ``}`} style={{ aspectRatio: `${size.w} / ${size.h}` }}>
        <span>No {platform} shot yet</span>
      </div>
    )
  return <img ref={ref} className={className} src={shotSrc(viewId, platform)} alt={alt} width={size.w} height={size.h} loading={eager ? `eager` : `lazy`} decoding="async" onError={() => setFailed(true)} />
}

export function ShotFigure({ viewId, platform, title, compact }: { viewId: string; platform: Platform; title: string; compact?: boolean }) {
  const phone = platform.id === `ios` || platform.id === `android`
  return (
    <figure className={`sdk-shot${phone ? ` is-phone` : ``}${compact ? ` is-compact` : ``}`}>
      <div className="sdk-shot-frame">
        <ShotImage viewId={viewId} platform={platform.id} alt={`${title} rendered by the ${platform.renderer} renderer on ${platform.label}`} />
      </div>
      <figcaption>
        <strong>{platform.label}</strong> <span>{platform.renderer}</span>
      </figcaption>
    </figure>
  )
}
