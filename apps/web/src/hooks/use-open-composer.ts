import { useCallback } from "react"
import {
  useLocation,
  useNavigate,
  useParams,
  useSearch,
} from "@tanstack/react-router"
import {
  searchFromSeed,
  type AgentSearch,
  type LaunchSeed,
} from "@/lib/launch-seed"
import {
  capturedOrigin,
  formatOrigin,
  parseOrigin,
  screenFromPath,
  type DetailOrigin,
} from "@/lib/detail-origin"

// EXP-825: "start something" is a NAVIGATION now — every play button (issue
// detail, the bulk bar, an action's Run, New action / a suggestion, a
// device's play, Reviews' Fix conflicts) sends the person to the Agent page
// composer with a preselection, the way `useOpenSession` sends them to a
// run. ONE place decides the URL shape.
//
// EXP-851: the composer also carries the ORIGIN the click came from
// (`?from=`), so the Agent page keeps that list nav beside it and the run it
// launches inherits it — starting from an issue detail keeps that issue's
// board list beside the run.
//
// EXP-870: a context-free caller (a PINNED action) passes `origin: null` —
// the composer then opens full-width with no list nav, exactly like a pinned
// issue or session does, instead of inheriting whatever list was up.

/** The composer's search params for a seed and an origin — pure, so the
 *  "no origin, no `from`" rule is a test. */
export function composerSearch(
  seed: Partial<LaunchSeed>,
  origin: DetailOrigin | null
): AgentSearch & { from?: string } {
  const token = formatOrigin(origin)
  return { ...searchFromSeed(seed), ...(token ? { from: token } : {}) }
}

export interface OpenComposerOptions {
  /** The list the click came from; `null` = context-free. Omit the KEY to
   *  derive it from the URL. */
  origin?: DetailOrigin | null
}

export function useOpenComposer(): (
  seed: Partial<LaunchSeed>,
  options?: OpenComposerOptions
) => void {
  const navigate = useNavigate()
  // Loose match: every caller lives under `/t/$teamSlug`, but the hook must
  // not throw in a story/test that mounts one outside the team layout.
  const { teamSlug } = useParams({ strict: false })
  const location = useLocation({ select: (current) => current.pathname })
  const from = useSearch({
    strict: false,
    select: (search) =>
      typeof (search as { from?: unknown }).from === `string`
        ? ((search as { from?: string }).from ?? null)
        : null,
  })

  return useCallback(
    (seed: Partial<LaunchSeed>, options?: OpenComposerOptions) => {
      if (!teamSlug) {
        console.warn(`useOpenComposer: no teamSlug in scope, ignoring`)
        return
      }
      const screen = screenFromPath(location ?? ``)
      const origin =
        options && `origin` in options
          ? (options.origin ?? null)
          : capturedOrigin(screen, parseOrigin(from))
      void navigate({
        to: `/t/$teamSlug/agent`,
        params: { teamSlug },
        search: composerSearch(seed, origin),
      })
    },
    [navigate, teamSlug, location, from]
  )
}
