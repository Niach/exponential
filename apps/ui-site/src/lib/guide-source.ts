import { GUIDE_SOURCES } from "../generated/guide-sources"

/** A guide example file's text by its path under guides/ (`react/src/App.tsx`). */
export function guideSource(path: string): string {
  const text = GUIDE_SOURCES[path]
  if (text === undefined) throw new Error(`guide source missing: guides/${path}`)
  return text
}
