/** EXP-1186: one team id, or several (a phone's cross-team surfaces), as
 *  a SORTED id list — the stable live-query key. */
export function teamScopeIds(
  teamId: string | readonly string[] | undefined
): string[] {
  if (teamId === undefined) return []
  if (typeof teamId === `string`) return teamId ? [teamId] : []
  return [...teamId].sort()
}
