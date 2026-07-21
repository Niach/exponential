// Custom Better Auth `additionalFields` are declared server-side in
// `lib/auth/index.ts` (isAdmin, onboardingCompletedAt). Better Auth's
// `getSession` type inference does not reliably surface these on the
// session-user type, so these typed accessors centralize the single structural
// read instead of scattering `as { isAdmin?: boolean }` casts across the client
// and server. Dependency-free on purpose so both sides can import it.

export interface AppUserFields {
  isAdmin?: boolean | null
  onboardingCompletedAt?: string | Date | null
  desktopAppCardDismissedAt?: string | Date | null
  gettingStartedDismissedAt?: string | Date | null
  // Index signature so concrete Better Auth user objects (which carry many
  // other fields) are assignable here without tripping weak-type detection.
  [key: string]: unknown
}

type MaybeAppUser = AppUserFields | null | undefined

/** Whether the user is a global admin. */
export function isAdminUser(user: MaybeAppUser): boolean {
  return Boolean(user?.isAdmin)
}

/** Whether the user has finished onboarding. */
export function hasCompletedOnboarding(user: MaybeAppUser): boolean {
  return user?.onboardingCompletedAt != null
}

/** Whether the user dismissed the "Getting started" cards (EXP-88). */
export function hasDismissedGettingStarted(user: MaybeAppUser): boolean {
  return user?.gettingStartedDismissedAt != null
}
