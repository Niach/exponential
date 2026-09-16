import { useEffect } from "react"

// EXP-883: every web page names itself in the tab. Titles read most-specific
// first and end on the product, the labels the page itself shows (sidebar,
// settings nav, admin heading): `Labels · Settings · Exponential`.
export const APP_TITLE = `Exponential`

export function pageTitle(...parts: (string | null | undefined)[]): string {
  return [...parts.map((part) => part?.trim()), APP_TITLE]
    .filter((part): part is string => Boolean(part))
    .join(` · `)
}

// A route's `head` title is static (params/search only). Pages whose name comes
// from synced data (an issue, a board, a thread) set it here once it resolves;
// until then the route's `head` fallback shows. The root `HeadContent` remounts
// `<title>` whenever the route's head title changes, so the effect re-applies
// on every change and, on unmount, hands back the head title only while ours
// still stands (a remounted element already carries the next route's).
export function usePageTitle(title: string | undefined): void {
  useEffect(() => {
    if (!title || typeof document === `undefined`) return
    const previous = document.title
    document.title = title
    return () => {
      if (document.title === title) document.title = previous
    }
  }, [title])
}
