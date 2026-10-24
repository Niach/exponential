/* The four shot platforms and the shot URL, as src/lib/catalog.ts states
   them, WITHOUT the catalog's data imports: the home page's demo uses these
   and must not pull ~300 KB of generated docs into its chunk.
   sdk.test.ts holds the two in lockstep. */
export const PLATFORMS = [
  { id: `web`, label: `Web`, renderer: `React` },
  { id: `ios`, label: `iOS`, renderer: `SwiftUI` },
  { id: `android`, label: `Android`, renderer: `Compose` },
  { id: `desktop`, label: `Desktop`, renderer: `gpui` },
] as const

export type ShotPlatform = (typeof PLATFORMS)[number]

export const shotSrc = (viewId: string, platform: string) => `/shots/${viewId}/${platform}.webp`
