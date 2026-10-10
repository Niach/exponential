// VAPP-91: the host router. Server messages in, ops out (`catalog/host.json`
// ops); the platform host performs them on its surfaces. Pure and
// synchronous, so fixtures/host-router.json locks the same behaviour on
// every platform (the Rust core's `host::HostRouter`, which Swift and Kotlin
// reach through the facade).

import { A2UI_VERSION } from "../catalog"
import { errorMessage, MESSAGE_KINDS } from "./contract"
import type { HostOp, ServerMessage } from "./contract"
import { supportedCatalogIds, templateMessages, validatePackage } from "./package"
import type { PackageIssue, VappPackage } from "./package"
import { parseSource } from "./sources"
import { LIMIT_ISSUES, MAX_MESSAGE_BYTES } from "../limits"

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)

export interface HostRouterOptions {
  /** Extension catalog ids the host registered (negotiation). */
  extensionIds?: readonly string[]
}

export interface SurfaceInfo {
  surfaceId: string
  catalogId: string
  /** The package whose template created it (its function policy). */
  packageId?: string
}

export class HostRouter {
  private extensionIds: string[]
  private surfaces = new Map<string, SurfaceInfo>()
  private packages = new Map<string, VappPackage>()

  constructor(options: HostRouterOptions = {}) {
    this.extensionIds = [...(options.extensionIds ?? [])]
  }

  get supportedCatalogIds(): string[] {
    return supportedCatalogIds(this.extensionIds)
  }

  registerExtension(id: string): void {
    if (!this.extensionIds.includes(id)) this.extensionIds.push(id)
  }

  /** Install a declarative package: its templates become `applyTemplate`
   *  targets. Returns the validation issues (installed only when none). */
  installPackage(pkg: VappPackage): PackageIssue[] {
    const issues = validatePackage(pkg, this.supportedCatalogIds)
    if (!issues.length) this.packages.set(pkg.id, pkg)
    return issues
  }

  package(id: string): VappPackage | undefined {
    return this.packages.get(id)
  }

  surface(id: string): SurfaceInfo | undefined {
    return this.surfaces.get(id)
  }

  surfaceIds(): string[] {
    return [...this.surfaces.keys()]
  }

  /** One server message → the ops to perform, in order. Never throws. */
  route(message: unknown): HostOp[] {
    // VAPP-103: a message past `maxMessageBytes` (as UTF-8 JSON) is refused.
    if (oversized(message)) return [this.invalid(isObject(message) ? surfaceIdOf(message) : ``, LIMIT_ISSUES.messageBytes)]
    if (!isObject(message)) return [this.invalid(``, `a message is a JSON object`)]
    if (message.version !== undefined && message.version !== A2UI_VERSION) return [this.invalid(surfaceIdOf(message), `unsupported version ${String(message.version)}`)]
    const kinds = Object.keys(message).filter((k) => k !== `version`)
    if (kinds.length !== 1 || !MESSAGE_KINDS.includes(kinds[0]!)) return [this.invalid(surfaceIdOf(message), `expected exactly one of ${MESSAGE_KINDS.join(`, `)}`)]
    const kind = kinds[0] as keyof ServerMessage
    const body = message[kind]
    if (!isObject(body) || typeof body.surfaceId !== `string` || !body.surfaceId) return [this.invalid(``, `${kind}.surfaceId is required`)]
    const surfaceId = body.surfaceId
    switch (kind) {
      case `createSurface`: {
        if (typeof body.catalogId !== `string`) return [this.invalid(surfaceId, `createSurface.catalogId is required`)]
        if (!this.supportedCatalogIds.includes(body.catalogId))
          return [{ op: `send`, message: errorMessage(`UNSUPPORTED_CATALOG`, surfaceId, `catalog ${body.catalogId} is not supported`) }]
        this.surfaces.set(surfaceId, { surfaceId, catalogId: body.catalogId })
        const op: HostOp = { op: `create`, surfaceId, catalogId: body.catalogId }
        if (body.theme !== undefined) op.theme = body.theme
        if (body.sendDataModel === true) op.sendDataModel = true
        return [op]
      }
      case `updateComponents`: {
        if (!Array.isArray(body.components)) return [this.invalid(surfaceId, `updateComponents.components is required`)]
        if (!this.surfaces.has(surfaceId)) return [this.missing(surfaceId)]
        return [{ op: `components`, surfaceId, components: body.components as never }]
      }
      case `updateDataModel`: {
        if (!this.surfaces.has(surfaceId)) return [this.missing(surfaceId)]
        if (body.path !== undefined && typeof body.path !== `string`) return [this.invalid(surfaceId, `updateDataModel.path is a string`)]
        const path = normalizePath(body.path as string | undefined)
        return [`value` in body ? { op: `data`, surfaceId, path, value: body.value } : { op: `data`, surfaceId, path }]
      }
      case `deleteSurface`: {
        if (!this.surfaces.has(surfaceId)) return [this.missing(surfaceId)]
        this.surfaces.delete(surfaceId)
        return [{ op: `delete`, surfaceId }]
      }
      case `bindDataModel`: {
        if (!this.surfaces.has(surfaceId)) return [this.missing(surfaceId)]
        if (typeof body.path !== `string` || typeof body.source !== `string` || !parseSource(body.source))
          return [this.invalid(surfaceId, `bindDataModel needs a path and a scheme:name source`)]
        return [{ op: `bind`, surfaceId, path: normalizePath(body.path), source: body.source }]
      }
      case `applyTemplate`: {
        if (typeof body.templateId !== `string`) return [this.invalid(surfaceId, `applyTemplate.templateId is required`)]
        const pkg = this.findPackage(body.templateId, typeof body.packageId === `string` ? body.packageId : undefined)
        if (!pkg) return [{ op: `send`, message: errorMessage(`TEMPLATE_NOT_FOUND`, surfaceId, `no installed package has the template ${body.templateId}`) }]
        const ops: HostOp[] = []
        for (const m of templateMessages(pkg, body.templateId, surfaceId, body.data) ?? []) ops.push(...this.route(m))
        const info = this.surfaces.get(surfaceId)
        if (info) info.packageId = pkg.id
        return ops
      }
    }
    return [this.invalid(surfaceId, `unknown message`)]
  }

  private findPackage(templateId: string, packageId?: string): VappPackage | undefined {
    if (packageId !== undefined) {
      const pkg = this.packages.get(packageId)
      return pkg && pkg.templates[templateId] ? pkg : undefined
    }
    for (const pkg of this.packages.values()) if (pkg.templates[templateId]) return pkg
    return undefined
  }

  private invalid(surfaceId: string, message: string): HostOp {
    return { op: `send`, message: errorMessage(`INVALID_MESSAGE`, surfaceId, message) }
  }

  private missing(surfaceId: string): HostOp {
    return { op: `send`, message: errorMessage(`SURFACE_NOT_FOUND`, surfaceId, `no surface ${surfaceId}; send createSurface first`) }
  }
}

/** Is the message's JSON past `maxMessageBytes`? (A string's UTF-8 size is
 *  at most 3 bytes per UTF-16 unit: the exact count only near the limit.) */
function oversized(message: unknown): boolean {
  let text: string | undefined
  try {
    text = JSON.stringify(message)
  } catch {
    return false
  }
  if (text === undefined || text.length * 3 <= MAX_MESSAGE_BYTES) return false
  return text.length > MAX_MESSAGE_BYTES || new TextEncoder().encode(text).length > MAX_MESSAGE_BYTES
}

function surfaceIdOf(message: Record<string, unknown>): string {
  for (const v of Object.values(message)) if (isObject(v) && typeof v.surfaceId === `string`) return v.surfaceId
  return ``
}

/** A2UI: an omitted path or `/` is the whole data model (`""` in JSON
 *  pointer terms, which every surface's setPointer takes). */
export function normalizePath(path: string | undefined): string {
  return path === undefined || path === `/` ? `` : path
}
