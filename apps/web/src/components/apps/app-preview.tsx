import { useEffect, useMemo } from "react"
import { useRouter } from "@tanstack/react-router"
import { DEFAULT_THEME_ID, type AppPackage } from "@exponential_app/ui"
import { HostSurface } from "@exponential_app/ui-react"
import { APPS_COPY, APPS_PREVIEW, APPS_TEST_IDS } from "@exp/domain-contract/apps"
import { appReactExtension, conceptIcon, EmptyState } from "@exp/ui"
import { ConsentCard, HostBanner, createAppHost, createConsentGate } from "@/lib/exponential-ui-host"
import { exponentialUiIcons } from "@/lib/exponential-ui-icons"

// VAPP-110: the editor's PREVIEW: the package's first template rendered
// through the app host (the same public host API a third party uses:
// `exp:` bindings, `harness.*` functions behind the consent card, the URL
// policy). A new package = a new host (the key), so every synced upsert
// repaints; no package yet = the empty state in the frame.

const NavAppsIcon = conceptIcon(`nav-apps`)
const EXTENSIONS = [appReactExtension]

function PackageSurface({ pkg, mode, appId, teamId, me }: { pkg: AppPackage; mode: `light` | `dark`; appId: string | null; teamId: string | null; me: string | null }) {
  const router = useRouter()
  const consent = useMemo(createConsentGate, [])
  const host = useMemo(
    () => createAppHost({ consent, navigate: (href) => router.history.push(href), packages: [pkg], appId, teamId, me }),
    [consent, router, pkg, appId, teamId, me]
  )
  const plugin = useMemo(() => ({ icons: exponentialUiIcons }), [])
  const templateId = Object.keys(pkg.templates)[0]
  useEffect(() => {
    if (templateId) {
      host.receive({
        version: `v0.9`,
        applyTemplate: { surfaceId: APPS_PREVIEW.surfaceId, templateId, packageId: pkg.id },
      })
    }
    return () => host.close()
  }, [host, pkg.id, templateId])
  return (
    <>
      <HostBanner host={host} />
      <HostSurface
        host={host}
        surfaceId={APPS_PREVIEW.surfaceId}
        plugin={plugin}
        extensions={EXTENSIONS}
        theme={typeof pkg.theme === `string` ? pkg.theme : DEFAULT_THEME_ID}
        mode={mode}
      />
      <ConsentCard gate={consent} />
    </>
  )
}

export function AppPreview({
  pkg,
  mode = `dark`,
  className,
  appId = null,
  teamId = null,
  me = null,
}: {
  pkg: Record<string, unknown> | null
  mode?: `light` | `dark`
  className?: string
  /** VAPP-122: the app whose records `exp:records` reads, and the viewer. */
  appId?: string | null
  teamId?: string | null
  me?: string | null
}) {
  // The key = the package's identity: a new version mounts a fresh host.
  const key = useMemo(() => (pkg ? JSON.stringify(pkg) : ``), [pkg])
  return (
    <div
      className={`relative min-h-0 flex-1 overflow-y-auto rounded-[12px] border border-border bg-background ${className ?? ``}`}
      style={{ borderRadius: APPS_PREVIEW.radius, padding: APPS_PREVIEW.padding }}
      data-testid={APPS_TEST_IDS.preview}
    >
      {pkg ? (
        <PackageSurface key={key} pkg={pkg as unknown as AppPackage} mode={mode} appId={appId} teamId={teamId} me={me} />
      ) : (
        <div className="flex h-full min-h-48 items-center justify-center">
          <EmptyState icon={NavAppsIcon} title={APPS_COPY.previewEmpty} size="page" />
        </div>
      )}
    </div>
  )
}
