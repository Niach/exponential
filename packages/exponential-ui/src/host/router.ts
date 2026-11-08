// VAPP-91: the host router. Agent messages in, ops out (`catalog/host.json`
// ops); the platform host performs them on its surfaces. Pure and
// synchronous, so fixtures/host-router.json locks the same behaviour on
// every platform (the Rust core's `host::HostRouter`, which Swift and Kotlin
// reach through the facade).
//
// Round 9 (A2UI 1.0, docs/round-9-contract.md §1–§2): the check order of §1.1
// (size, object, version through `checkEnvelope` with the 0.9 bridge, one
// known kind, host-internal kinds only TRUSTED, closed bodies, the kind's own
// checks), lifetime-unique surface ids (INTEGRITY_ERROR, `mintSurfaceId`),
// `createSurface{components, dataModel, metadata}`, the per-entry check (§5.2:
// a malformed entry is dropped with ONE INVALID_MESSAGE, the rest applies),
// `callRendererFunction` (§6.9) and `agentFunctionResponse`.

import { checkArgs, allowedCallers, declaredFunction, INDEX_FUNCTION, requiresUserActivation } from "../functions"
import { ACCEPT_V09, A2UI_VERSION } from "../catalog"
import { checkEntry, checkEnvelope, checkMetadata, entryRefusal } from "../entry"
import type { ExtensionDef } from "../types"
import { emptyUpgrade09State, upgradeMessage09, upgradePackageBody09 } from "../upgrade09"
import { A2UI_MESSAGE_KINDS, errorMessage, HOST_MESSAGE_KINDS, MESSAGE_KINDS, rendererFunctionResponse } from "./contract"
import type { HostErrorCode, HostOp } from "./contract"
import { isRecordMacroBinding } from "./record-macros"
import { sourceAllowed, supportedCatalogIds, templateMessages, validatePackage } from "./package"
import type { AvailableExtension, PackageIssue, AppPackage } from "./package"
import { parseSource } from "./sources"
import { fillHostParams } from "./host-params"
import { expandQuerySource } from "./query"
import { LIMIT_ISSUES, MAX_MESSAGE_BYTES } from "../limits"

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)

export interface HostRouterOptions {
  /** Extension catalog ids the host registered (negotiation). */
  extensionIds?: readonly string[]
  /** VAPP-161: their versions and components (a package's `extensions`
   *  are checked against them; unset = only that the id is registered). */
  extensions?: readonly AvailableExtension[]
}

export interface SurfaceInfo {
  surfaceId: string
  /** The surface's default catalog (absent = none, §5.5). */
  catalogId?: string
  /** The package whose template created it (its function policy). */
  packageId?: string
  /** §6.5: created by an UNTRUSTED `createSurface` (an agent's). */
  agent: boolean
}

/** `route`'s options (§1.1): `trusted` = host code (applyTemplate,
 *  bindDataModel allowed); `acceptV09` = run the 0.9 bridge. */
export interface RouteOptions {
  trusted?: boolean
  acceptV09?: boolean
}

/** The body keys each kind allows (§1.2; the host kinds also carry the
 *  internal `hostParams`). */
const ALLOWED_KEYS: Record<string, readonly string[]> = {
  createSurface: [`surfaceId`, `catalogId`, `components`, `dataModel`, `sendDataModel`, `metadata`],
  updateComponents: [`surfaceId`, `components`],
  updateDataModel: [`surfaceId`, `path`, `value`],
  deleteSurface: [`surfaceId`],
  callRendererFunction: [`surfaceId`, `functionCallId`, `callFunction`],
  agentFunctionResponse: [`functionCallId`, `value`, `error`],
  applyTemplate: [`surfaceId`, `templateId`, `packageId`, `data`, `hostParams`],
  bindDataModel: [`surfaceId`, `path`, `source`, `hostParams`],
}

const send = (code: HostErrorCode, target: Parameters<typeof errorMessage>[1], message: string, path?: string): HostOp => ({ op: `send`, message: errorMessage(code, target, message, path) })

export class HostRouter {
  private extensionIds: string[]
  /** §10: inline catalog ids (they join the available set). */
  private inlineIds: string[] = []
  private surfaces = new Map<string, SurfaceInfo>()
  /** §1.4: every surface id ever created by this router. */
  private seen = new Set<string>()
  /** §1.4: the mint counter. */
  private mintNext = 1
  private packages = new Map<string, AppPackage>()
  private extensions: AvailableExtension[] | undefined
  /** §14: the 0.9 bridge's id maps, carried across the stream. */
  private upgradeState = emptyUpgrade09State()

  constructor(options: HostRouterOptions = {}) {
    this.extensionIds = [...(options.extensionIds ?? [])]
    this.extensions = options.extensions ? [...options.extensions] : undefined
  }

  get supportedCatalogIds(): string[] {
    return supportedCatalogIds(this.extensionIds)
  }

  /** §5.5: the AVAILABLE set: the supported ids then the inline catalogs. */
  get availableCatalogIds(): string[] {
    const out = this.supportedCatalogIds
    for (const id of this.inlineIds) if (!out.includes(id)) out.push(id)
    return out
  }

  /** §10: an inline catalog the agent sent joins the available set. */
  registerInlineCatalog(id: string): void {
    if (!this.inlineIds.includes(id)) this.inlineIds.push(id)
  }

  registerExtension(id: string, available?: AvailableExtension): void {
    if (!this.extensionIds.includes(id)) this.extensionIds.push(id)
    if (this.extensions && !this.extensions.some((e) => e.id === id)) this.extensions.push(available ?? { id })
  }

  /** Install a declarative package: its templates become `applyTemplate`
   *  targets. Returns the validation issues (installed only when none).
   *  Round 9 §12: a format-5 package is upgraded while the bridge is on. */
  installPackage(pkg: AppPackage): PackageIssue[] {
    const upgraded = bridgePackage(pkg)
    const issues = validatePackage(upgraded, this.supportedCatalogIds, this.extensions)
    if (!issues.length) this.packages.set(upgraded.id, upgraded)
    return issues
  }

  package(id: string): AppPackage | undefined {
    return this.packages.get(id)
  }

  surface(id: string): SurfaceInfo | undefined {
    return this.surfaces.get(id)
  }

  surfaceIds(): string[] {
    return [...this.surfaces.keys()]
  }

  /** Has this router ever created `surfaceId`? */
  wasCreated(surfaceId: string): boolean {
    return this.seen.has(surfaceId)
  }

  /** §1.4: a fresh id for a re-opened surface: `<base>#<n>`, skipping ids
   *  seen or live. Minting does not add to `seen`; creating does. */
  mintSurfaceId(base: string): string {
    for (;;) {
      const candidate = `${base}#${this.mintNext}`
      this.mintNext += 1
      if (!this.seen.has(candidate) && !this.surfaces.has(candidate)) return candidate
    }
  }

  /** One agent message → the ops to perform, in order. Never throws. */
  route(message: unknown, options: RouteOptions = {}): HostOp[] {
    const acceptV09 = options.acceptV09 ?? ACCEPT_V09
    // VAPP-103: a message past `maxMessageBytes` (as UTF-8 JSON) is refused.
    if (oversized(message)) return [this.invalid(isObject(message) ? surfaceIdOf(message) : ``, LIMIT_ISSUES.messageBytes)]
    if (!isObject(message)) return [this.invalid(``, `a message is a JSON object`)]
    const envelope = checkEnvelope(message, acceptV09)
    if (envelope.kind === `refused`) return [this.invalid(surfaceIdOf(message), envelope.reason)]
    if (envelope.kind === `upgrade`) {
      const { messages, state } = upgradeMessage09(message, this.upgradeState)
      this.upgradeState = state
      const ops: HostOp[] = []
      for (const m of messages) {
        // The upgraded form is 1.0: never bridged twice.
        if (!isObject(m) || m.version !== A2UI_VERSION) ops.push(this.invalid(isObject(m) ? surfaceIdOf(m) : ``, `upgrade09 produced no 1.0 message`))
        else ops.push(...this.route(m, options))
      }
      return ops
    }
    const sid = surfaceIdOf(message)
    const kinds = Object.keys(message).filter((k) => k !== `version`)
    if (kinds.length !== 1 || !MESSAGE_KINDS.includes(kinds[0]!)) return [this.invalid(sid, `expected exactly one of ${A2UI_MESSAGE_KINDS.join(`, `)}`)]
    const kind = kinds[0]!
    if (HOST_MESSAGE_KINDS.includes(kind) && !options.trusted) return [this.invalid(sid, `${kind} is host-internal`)]
    const body = message[kind]
    if (!isObject(body)) return [this.invalid(sid, `${kind} is an object`)]
    const stray = Object.keys(body).find((k) => !ALLOWED_KEYS[kind]!.includes(k))
    if (stray !== undefined) return [this.invalid(sid, `${kind}.${stray} is not allowed`)]
    if (kind === `callRendererFunction`) return this.callRendererFunction(body)
    if (kind === `agentFunctionResponse`) return agentFunctionResponse(body)
    if (typeof body.surfaceId !== `string` || !body.surfaceId) return [this.invalid(sid, `${kind}.surfaceId is required`)]
    const surfaceId = body.surfaceId
    switch (kind) {
      case `createSurface`:
        return this.createSurface(surfaceId, body, options)
      case `updateComponents`: {
        if (!Array.isArray(body.components) || body.components.length === 0) return [this.invalid(surfaceId, `updateComponents.components needs at least one component`)]
        const info = this.surfaces.get(surfaceId)
        if (!info) return [this.missing(surfaceId)]
        return this.componentOps(surfaceId, `updateComponents`, body.components, info.catalogId)
      }
      case `updateDataModel`: {
        if (!this.surfaces.has(surfaceId)) return [this.missing(surfaceId)]
        if (!(`value` in body)) return [this.invalid(surfaceId, `updateDataModel.value is required (null deletes)`)]
        if (body.path !== undefined && typeof body.path !== `string`) return [this.invalid(surfaceId, `updateDataModel.path is a string`)]
        const path = normalizePath(body.path as string | undefined)
        return [body.value === null ? { op: `data`, surfaceId, path } : { op: `data`, surfaceId, path, value: body.value }]
      }
      case `deleteSurface`: {
        if (!this.surfaces.has(surfaceId)) return [this.missing(surfaceId)]
        this.surfaces.delete(surfaceId)
        return [{ op: `delete`, surfaceId }]
      }
      case `bindDataModel`: {
        const info = this.surfaces.get(surfaceId)
        if (!info) return [this.missing(surfaceId)]
        if (typeof body.path !== `string` || typeof body.source !== `string` || !parseSource(body.source))
          return [this.invalid(surfaceId, `bindDataModel needs a path and a scheme:name source`)]
        // VAPP-107: a package surface follows only the sources its package lists.
        const pkg = info.packageId ? this.packages.get(info.packageId) : undefined
        if (pkg && !sourceAllowed(pkg.sources, body.source) && !isRecordMacroBinding(normalizePath(body.path), body.source))
          return [send(`SOURCE_DENIED`, surfaceId, `source ${body.source} is not in package ${pkg.id}'s sources`, normalizePath(body.path) || `/`)]
        // VAPP-190: `?query=<name>` follows the package's named query.
        // VAPP-206: `${today}`, `${now}`, `${month}` from the host's params.
        const host = isObject(body.hostParams) ? (body.hostParams as Record<string, string>) : undefined
        return [{ op: `bind`, surfaceId, path: normalizePath(body.path), source: pkg ? expandQuerySource(fillHostParams(body.source, host), pkg.queries, host) : body.source }]
      }
      case `applyTemplate`:
        return this.applyTemplate(surfaceId, body, options)
    }
    return [this.invalid(surfaceId, `unknown message`)]
  }

  /** §1.2 createSurface. */
  private createSurface(surfaceId: string, body: Record<string, unknown>, options: RouteOptions): HostOp[] {
    const catalogId = body.catalogId
    if (catalogId !== undefined && typeof catalogId !== `string`) return [this.invalid(surfaceId, `createSurface.catalogId is a string`)]
    if (catalogId !== undefined && !this.availableCatalogIds.includes(catalogId)) return [send(`UNSUPPORTED_CATALOG`, surfaceId, `catalog ${catalogId} is not supported`)]
    if (this.seen.has(surfaceId)) return [send(`INTEGRITY_ERROR`, surfaceId, `surface ${surfaceId} was already created; surface ids are unique for the renderer's lifetime`)]
    if (body.components !== undefined && (!Array.isArray(body.components) || body.components.length === 0)) return [this.invalid(surfaceId, `createSurface.components needs at least one component`)]
    if (body.dataModel !== undefined && !isObject(body.dataModel)) return [this.invalid(surfaceId, `createSurface.dataModel is an object`)]
    if (body.sendDataModel !== undefined && typeof body.sendDataModel !== `boolean`) return [this.invalid(surfaceId, `createSurface.sendDataModel is a boolean`)]
    if (body.metadata !== undefined) {
      const bad = checkMetadata(body.metadata, ``)
      if (bad) return [send(`INVALID_MESSAGE`, surfaceId, `createSurface.${bad.message}`, `/createSurface/metadata${bad.pointer}`)]
    }
    this.seen.add(surfaceId)
    this.surfaces.set(surfaceId, { surfaceId, ...(catalogId === undefined ? {} : { catalogId }), agent: !options.trusted })
    const create: Extract<HostOp, { op: `create` }> = { op: `create`, surfaceId }
    if (catalogId !== undefined) create.catalogId = catalogId
    if (body.sendDataModel === true) create.sendDataModel = true
    if (body.metadata !== undefined) {
      create.metadata = body.metadata as { extensions?: Record<string, unknown> }
      // §8: the theme rides in metadata.extensions.
      const theme = (body.metadata as { extensions?: Record<string, unknown> }).extensions?.at_exponential_theme
      if (theme !== undefined) create.theme = theme
    }
    const ops: HostOp[] = [create]
    if (body.dataModel !== undefined) ops.push({ op: `data`, surfaceId, path: ``, value: body.dataModel })
    if (body.components !== undefined) ops.push(...this.componentOps(surfaceId, `createSurface`, body.components as unknown[], catalogId))
    return ops
  }

  /** §1.2 / §5.2: every entry through `checkEntry`: one INVALID_MESSAGE per
   *  refused entry (in order), then the `components` op of the accepted ones. */
  private componentOps(surfaceId: string, kind: string, components: readonly unknown[], catalogId: string | undefined): HostOp[] {
    const ops: HostOp[] = []
    const accepted: unknown[] = []
    components.forEach((entry, i) => {
      const failure = checkEntry(entry, { catalogId, extensions: this.extensionDefs() })
      if (failure) {
        const { path, message } = entryRefusal(kind, i, failure)
        ops.push(send(`INVALID_MESSAGE`, surfaceId, message, path))
      } else accepted.push(entry)
    })
    if (accepted.length > 0) ops.push({ op: `components`, surfaceId, components: accepted as never })
    return ops
  }

  private extensionDefs(): ExtensionDef[] {
    return (this.extensions ?? []).flatMap((e) => (e.def ? [e.def] : []))
  }

  /** §2: applyTemplate expands into one createSurface plus binds, each routed trusted. */
  private applyTemplate(surfaceId: string, body: Record<string, unknown>, options: RouteOptions): HostOp[] {
    if (typeof body.templateId !== `string`) return [this.invalid(surfaceId, `applyTemplate.templateId is required`)]
    const pkg = this.findPackage(body.templateId, typeof body.packageId === `string` ? body.packageId : undefined)
    if (!pkg) return [send(`TEMPLATE_NOT_FOUND`, surfaceId, `no installed package has the template ${body.templateId}`)]
    if (body.data !== undefined && !isObject(body.data)) return [this.invalid(surfaceId, `applyTemplate.data is an object`)]
    const ops: HostOp[] = []
    for (const m of templateMessages(pkg, body.templateId, surfaceId, body.data) ?? []) {
      // VAPP-206: the template's own binds read the host's params too.
      const bind = (m as { bindDataModel?: Record<string, unknown> }).bindDataModel
      if (bind && isObject(body.hostParams)) bind.hostParams = body.hostParams
      const created = m.createSurface !== undefined
      const routed = this.route(m, { ...options, trusted: true })
      ops.push(...routed)
      if (created && !routed.some((op) => op.op === `create`)) break
      // The surface belongs to the package from its creation on (its
      // function policy and source allowlist apply to the template's own binds).
      const info = this.surfaces.get(surfaceId)
      if (info) info.packageId = pkg.id
    }
    return ops
  }

  /** §6.9 callRendererFunction. */
  private callRendererFunction(body: Record<string, unknown>): HostOp[] {
    const id = body.functionCallId
    if (typeof id !== `string` || !id) return [this.invalid(``, `callRendererFunction.functionCallId is required`)]
    const bad = (message: string): HostOp[] => [send(`INVALID_MESSAGE`, { functionCallId: id }, message)]
    const answer = (code: string, message: string): HostOp[] => [{ op: `send`, message: rendererFunctionResponse(id, { error: { code, message } }) }]
    const call = body.callFunction
    if (!isObject(call)) return bad(`callRendererFunction.callFunction is a function call`)
    const name = call[`@call`]
    const catalogId = call.catalogId
    if (typeof name !== `string` || typeof catalogId !== `string`) return bad(`callRendererFunction.callFunction needs a string @call and catalogId`)
    const stray = Object.keys(call).find((k) => k !== `@call` && k !== `args` && k !== `catalogId`)
    if (stray !== undefined) return bad(`callRendererFunction.callFunction.${stray} is not allowed`)
    if (call.args !== undefined && !isObject(call.args)) return bad(`callRendererFunction.callFunction.args is an object`)
    const surfaceId = body.surfaceId
    if (surfaceId !== undefined && typeof surfaceId !== `string`) return bad(`callRendererFunction.surfaceId is a string`)
    if (!this.availableCatalogIds.includes(catalogId)) return answer(`INVALID_FUNCTION_CALL`, `catalog ${catalogId} is not available`)
    if (surfaceId !== undefined && !this.surfaces.has(surfaceId)) return answer(`INVALID_FUNCTION_CALL`, `no surface ${surfaceId}`)
    if (name === INDEX_FUNCTION) return answer(`EXECUTION_ERROR`, `@index outside a template`)
    const signature = declaredFunction(catalogId, name)
    if (!signature) return answer(`INVALID_FUNCTION_CALL`, `unknown function ${name}`)
    if (allowedCallers(name) === `rendererOnly`) return answer(`INVALID_FUNCTION_CALL`, `${name} cannot be called by the agent`)
    if (requiresUserActivation(name)) return answer(`INVALID_FUNCTION_CALL`, `${name} needs a user gesture`)
    const args = isObject(call.args) ? call.args : {}
    const issue = checkArgs(signature, args)
    if (issue !== undefined) return answer(`INVALID_FUNCTION_CALL`, `args: ${issue}`)
    return [{ op: `call`, functionCallId: id, ...(surfaceId === undefined ? {} : { surfaceId }), catalogId, name, args }]
  }

  private findPackage(templateId: string, packageId?: string): AppPackage | undefined {
    if (packageId !== undefined) {
      const pkg = this.packages.get(packageId)
      return pkg && pkg.templates[templateId] ? pkg : undefined
    }
    for (const pkg of this.packages.values()) if (pkg.templates[templateId]) return pkg
    return undefined
  }

  private invalid(surfaceId: string, message: string): HostOp {
    return send(`INVALID_MESSAGE`, surfaceId, message)
  }

  private missing(surfaceId: string): HostOp {
    return send(`SURFACE_NOT_FOUND`, surfaceId, `no surface ${surfaceId}; send createSurface first`)
  }
}

/** §1.2 agentFunctionResponse. */
function agentFunctionResponse(body: Record<string, unknown>): HostOp[] {
  const id = body.functionCallId
  if (typeof id !== `string` || !id) return [send(`INVALID_MESSAGE`, ``, `agentFunctionResponse.functionCallId is required`)]
  const bad = (message: string): HostOp[] => [send(`INVALID_MESSAGE`, { functionCallId: id }, message)]
  const hasValue = `value` in body
  const hasError = `error` in body
  if (hasValue === hasError) return bad(`agentFunctionResponse needs exactly one of value and error`)
  if (hasValue) return [{ op: `respond`, functionCallId: id, value: body.value }]
  const e = body.error
  if (!isObject(e) || typeof e.code !== `string` || typeof e.message !== `string` || Object.keys(e).some((k) => k !== `code` && k !== `message`)) return bad(`agentFunctionResponse.error is {code, message}`)
  return [{ op: `respond`, functionCallId: id, error: { code: e.code, message: e.message } }]
}

/** Round 9 §12: a `formatVersion` 5 package body through `upgradePackageBody09`
 *  while `ACCEPT_V09` holds (anything else comes back as it is). */
export function bridgePackage<T>(pkg: T): T {
  return ACCEPT_V09 && isObject(pkg) && pkg.formatVersion === 5 ? (upgradePackageBody09(pkg) as T) : pkg
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

/** §1.1: the first string `surfaceId` among the message's object values, else `""`. */
export function surfaceIdOf(message: Record<string, unknown>): string {
  for (const v of Object.values(message)) if (isObject(v) && typeof v.surfaceId === `string`) return v.surfaceId
  return ``
}

/** A2UI: an omitted path or `/` is the whole data model (`""` in JSON
 *  pointer terms, which every surface's setPointer takes). */
export function normalizePath(path: string | undefined): string {
  return path === undefined || path === `/` ? `` : path
}
