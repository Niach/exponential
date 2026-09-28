import { createFileRoute } from "@tanstack/react-router"
import { redeemInstallTokenRequest } from "@/lib/auth/cli-install-token-redeem"

// EXP-1111: ANONYMOUS by design — the one-time `expi_` token IS the
// credential. The freshly installed `exponential` daemon trades it here for a
// Better Auth session token (`lib/auth/cli-install-token-redeem.ts`).
export const Route = createFileRoute(`/api/cli/install-token/redeem`)({
  server: {
    handlers: {
      POST: ({ request }) => redeemInstallTokenRequest(request),
    },
  },
})
