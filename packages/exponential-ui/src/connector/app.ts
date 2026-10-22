// VAPP-91: running a declarative app (VAPP-82) in any host: install the
// package, apply a template onto a surface, bind its sources (the host's own
// resolvers, or the Exponential connector's `exp:` ones), and let the
// package's `functions` list narrow the policy. The apps product's own host
// plugin is this plus its theme and extension.

import type { ServerMessage } from "../host/contract"
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
  /** Apply a template of the package onto a surface (default: the first). */
  open(surfaceId: string, templateId?: string, data?: unknown): void
}

export function createAppHost(options: AppHostOptions): AppHost {
  const { package: pkg, ...rest } = options
  const host = new ExponentialHost(rest)
  const issues = host.installPackage(pkg)
  return {
    host,
    issues,
    open(surfaceId, templateId = Object.keys(pkg.templates)[0]!, data) {
      const message: ServerMessage = { version: `v0.9`, applyTemplate: { surfaceId, templateId, packageId: pkg.id, ...(data === undefined ? {} : { data }) } }
      host.receive(message)
    },
  }
}
