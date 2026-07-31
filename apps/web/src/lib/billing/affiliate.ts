// Creem affiliate attribution (EXP-384). Hosted checkout normally attributes
// a sale via a first-party cookie on Creem's domain, set when the visitor
// clicked the affiliate link. That cookie can die between signup and checkout
// (Safari bounce-tracking purges, weeks-long gaps), so the signed `creem_ref`
// click token is persisted on the user row at signup
// (users.claimSignupAttribution) and re-appended to the checkout URL here —
// Creem honors an explicitly set `creem_ref` and never overwrites one, and a
// stale token is simply ignored.
export function withCreemRef(
  url: string,
  creemRef: string | null | undefined
): string {
  if (!creemRef) return url
  try {
    const target = new URL(url)
    if (target.searchParams.has(`creem_ref`)) return url
    target.searchParams.set(`creem_ref`, creemRef)
    return target.href
  } catch {
    return url
  }
}
