// Display-name resolution for user references. The users shape only syncs
// co-members of PRIVATE teams the viewer has joined — on public boards
// (and for any other unsynced user) the row is absent by design, so every
// render site needs a fallback that (a) never leaks the raw user id and
// (b) stays deterministic, so the same person reads as the same handle across
// comments, events and member lists.

export function anonymousUserLabel(userId: string): string {
  return `Member ${userId.slice(-4).toUpperCase()}`
}

// Chrome surfaces (sidebar footer, desktop rail) show only the first name;
// the full name + email live in account settings (EXP-311).
export function firstName(name: string): string {
  return name.trim().split(/\s+/)[0] ?? name
}

export function displayUserName(
  user: { name?: string | null; email?: string | null } | undefined,
  userId: string | null | undefined
): string {
  if (user?.name) return user.name
  if (user?.email) return user.email
  if (userId) return anonymousUserLabel(userId)
  return `Someone`
}
