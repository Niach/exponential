import { createFileRoute } from "@tanstack/react-router"
import { oAuthDiscoveryMetadata } from "better-auth/plugins"
import { auth } from "@/lib/auth"

// EXP-1153: OpenID Connect discovery at the root. The document better-auth's
// mcp plugin publishes under /.well-known/oauth-authorization-server is
// already OIDC-shaped (userinfo_endpoint, jwks_uri, id_token_signing_alg_
// values_supported, subject_types_supported), and ChatGPT Enterprise's
// workspace domain restriction needs it under THIS name too: it reads the
// `email` + `email_verified` claims off the userinfo endpoint it discovers
// here to keep a corporate identity from linking the plugin outside its
// workspace (developers.openai.com/plugins/build/auth#support-workspace-
// domain-restrictions). Same handler, second name.
const handler = oAuthDiscoveryMetadata(auth)

export const Route = createFileRoute(`/.well-known/openid-configuration`)({
  server: {
    handlers: {
      GET: ({ request }) => handler(request),
    },
  },
})
