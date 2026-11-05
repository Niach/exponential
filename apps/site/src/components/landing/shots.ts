/* Shot store pictures (shots/<view>/<platform>.webp) at their capture size. */
const FRAMES = { desktop: [1800, 1126], web: [1800, 1200], ios: [828, 1800], android: [810, 1800], "web-mobile": [828, 1800], ede: [1800, 1013] } as const
export type ShotPlatform = keyof typeof FRAMES

export function shot(view: string, platform: ShotPlatform, alt: string) {
  const [width, height] = FRAMES[platform]
  return { src: `/shots/${view}/${platform}.webp`, width, height, alt }
}
