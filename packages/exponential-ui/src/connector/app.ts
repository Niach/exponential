// VAPP-91: running a declarative app (VAPP-82) in any host: install the
// package, apply a template onto a surface, bind its sources (the host's own
// resolvers, or the Exponential connector's `exp:` ones), and let the
// package's `functions` list narrow the policy. The apps product's own host
// plugin is this plus its theme and extension.

import { A2UI_VERSION } from "../catalog"
import type { PackageIssue, AppPackage } from "../host/package"
import { ExponentialHost } from "../host/runtime"
import type { HostOptions } from "../host/runtime"

export interface AppHostOptions extends HostOptions {
  /** The package to run. */
  package: AppPackage
}

export interface AppHost {
  host: ExponentialHost
  issues: PackageIssue[]
  /** Apply a template of the package onto a surface (default: the first).
   *  Answers the surface's id: a re-opened surface gets a fresh one (round 9
   *  §1.4) and the old one goes once the new one exists. */
  open(surfaceId: string, templateId?: string, data?: unknown): string
}

export function createAppHost(options: AppHostOptions): AppHost {
  const { package: pkg, ...rest } = options
  const host = new ExponentialHost(rest)
  const issues = host.installPackage(pkg)
  return {
    host,
    issues,
    open(surfaceId, templateId = Object.keys(pkg.templates)[0]!, data) {
      const fresh = host.freshSurfaceId(surfaceId)
      host.applyTemplate({ surfaceId: fresh, templateId, packageId: pkg.id, ...(data === undefined ? {} : { data }) })
      if (fresh !== surfaceId && host.surface(surfaceId)) host.receive({ version: A2UI_VERSION, deleteSurface: { surfaceId } })
      return fresh
    },
  }
}
