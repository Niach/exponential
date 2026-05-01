// Pure helper extracted out of auth.ts so the client bundle can read OIDC
// provider config without transitively pulling in better-auth (whose
// tanstack-start integration does `await import("@tanstack/react-start/server")`,
// which drags `@tanstack/start-server-core` into the client bundle).

export type OidcProviderConfig = {
  id: string
  name: string
  clientId: string
  clientSecret: string
  discoveryUrl: string
  scopes?: string[]
  adminGroups?: string[]
  groupsClaim?: string
}

export function parseOidcProviders(): OidcProviderConfig[] {
  const raw = process.env.OIDC_PROVIDERS
  if (raw && raw.trim()) {
    try {
      const parsed = JSON.parse(raw) as Array<Partial<OidcProviderConfig>>
      if (!Array.isArray(parsed)) {
        console.error(`OIDC_PROVIDERS must be a JSON array`)
        return []
      }
      return parsed
        .filter((p) => p.id && p.clientId && p.clientSecret && p.discoveryUrl)
        .map((p) => ({
          id: p.id!,
          name: p.name || p.id!,
          clientId: p.clientId!,
          clientSecret: p.clientSecret!,
          discoveryUrl: p.discoveryUrl!,
          scopes: p.scopes,
          adminGroups: Array.isArray(p.adminGroups)
            ? p.adminGroups.filter((g): g is string => typeof g === `string`)
            : undefined,
          groupsClaim:
            typeof p.groupsClaim === `string` ? p.groupsClaim : undefined,
        }))
    } catch (err) {
      console.error(`Failed to parse OIDC_PROVIDERS env var:`, err)
      return []
    }
  }

  if (
    process.env.AUTH_OIDC_ENABLED === `true` &&
    process.env.OIDC_CLIENT_ID &&
    process.env.OIDC_CLIENT_SECRET &&
    process.env.OIDC_DISCOVERY_URL
  ) {
    const id = process.env.OIDC_PROVIDER_ID || `authentik`
    return [
      {
        id,
        name: id.charAt(0).toUpperCase() + id.slice(1),
        clientId: process.env.OIDC_CLIENT_ID,
        clientSecret: process.env.OIDC_CLIENT_SECRET,
        discoveryUrl: process.env.OIDC_DISCOVERY_URL,
      },
    ]
  }
  return []
}
