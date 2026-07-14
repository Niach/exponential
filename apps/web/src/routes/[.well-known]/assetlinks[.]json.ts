import { createFileRoute } from "@tanstack/react-router"
import { buildAssetLinks, parseFingerprints } from "@/lib/app-links"

// Android App Links (EXP-92). The SHA-256 signing-cert fingerprints live in
// the env (Play Console → App integrity → app signing key — Play App Signing
// means the shipping cert is never in the repo). Unset ⇒ 404: Android link
// verification fails and links keep opening in the browser, which is also the
// correct self-hosted default.
export const Route = createFileRoute(`/.well-known/assetlinks.json`)({
  server: {
    handlers: {
      GET: () => {
        const statements = buildAssetLinks(
          parseFingerprints(process.env.ANDROID_APP_LINK_FINGERPRINTS)
        )
        if (!statements) {
          return new Response(`Not found`, { status: 404 })
        }
        return new Response(JSON.stringify(statements), {
          headers: {
            "Content-Type": `application/json`,
            "Cache-Control": `public, max-age=3600`,
          },
        })
      },
    },
  },
})
