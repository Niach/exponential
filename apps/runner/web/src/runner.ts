// VAPP-269 (E6d): the web runner, runner.json's contract on the TS twin. ONE
// Engine AppInstance per page; the painter is genUI's ExponentialHost (the
// React host paints its surfaces). Every pixel is a genUI surface: `app`
// (the open screen, wrapped by `runnerPackage`), `consent`, `permissions`,
// `toast`, and `runnerFailure` in place of `app`. No server, no OTA, no
// telemetry, no account: the package comes from the bundle dir `app/`.
//
// Browser-free: the page wiring (events, storage, capabilities) is
// `page.ts`; tests drive this with fakes.

import { CORE_CATALOG_ID, ExponentialHost, type AppPackage, type ClientMessage, type HostFunction, type ServerMessage } from "@exponential_app/eui"
import {
  AppInstance,
  CAPABILITY_TOOLS,
  LoadError,
  RUNNER_BUNDLE,
  createsOf,
  localDeviceId,
  packageOf,
  runnerConfig,
  runnerFailure,
  runnerPackage,
  runnerToast,
  type BlobStore,
  type CallResult,
  type ConsentAnswer,
  type EngineRecordStore,
  type FormFactor,
  type LifecycleEvent,
  type NativeCapability,
  type RunnerConfig,
  type RunnerFailure,
  type SurfaceComponent,
  type Vault,
} from "@exp/eengine"
import lifecycle from "@exp/eengine/fixtures/lifecycle.json" with { type: "json" }
import runnerFixture from "@exp/eengine/fixtures/runner.json" with { type: "json" }

const VERSION = `v0.9`
const RECORDS_FUNCTIONS = [`records.upsert`, `records.delete`, `records.batch`]
/** A toast's own duration (genUI `Toast.duration` default) plus its exit. */
const TOAST_MS = 5_500

export interface RunnerStorage {
  records: EngineRecordStore
  vault: Vault
  blobs: BlobStore
}

export interface RunnerOptions {
  /** A bundle file under `app/` (runner.json, package.app.json); null = absent. */
  readBundle(name: string): Promise<string | null>
  /** The stores of one app id (runner.json `storage`). */
  openStorage(appId: string): Promise<RunnerStorage>
  capabilities: readonly NativeCapability[]
  formFactor: FormFactor
  locale?: string
  newId?: () => string
  setTimeout?: (fn: () => void, ms: number) => unknown
  clearTimeout?: (timer: unknown) => void
}

type Pending = { name: string; args: unknown; ask: Extract<CallResult, { ask: unknown }>[`ask`]; settle: (result: Promise<unknown>) => void }

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)

/** `?screen=x&k=v` or `#x?k=v` → a deep link (null = a plain launch). */
export function deepLinkOf(url: URL): Extract<LifecycleEvent, { type: `deepLink` }> | null {
  const search = new URLSearchParams(url.search)
  let screen = search.get(`screen`)
  let params: URLSearchParams = search
  if (!screen && url.hash.length > 1) {
    const hash = url.hash.slice(1).replace(/^\//, ``)
    const at = hash.indexOf(`?`)
    screen = decodeURIComponent(at < 0 ? hash : hash.slice(0, at))
    params = new URLSearchParams(at < 0 ? `` : hash.slice(at + 1))
  }
  if (!screen) return null
  const out: Record<string, string> = {}
  for (const [k, v] of params) if (k !== `screen`) out[k] = v
  return { type: `deepLink`, screen, params: out }
}

export class Runner {
  readonly host: ExponentialHost
  config: RunnerConfig | null = null
  instance: AppInstance | null = null
  failure: RunnerFailure | null = null
  private formFactor: FormFactor
  private pending: Pending | null = null
  private toastTimer: unknown = null
  private ids: () => string

  constructor(private readonly options: RunnerOptions) {
    this.formFactor = options.formFactor
    this.ids = options.newId ?? (() => crypto.randomUUID())
    this.host = new ExponentialHost({
      functions: this.functions(),
      sources: {
        records: (source, emit) => {
          const instance = this.instance
          if (!instance) return void emit(null)
          let live = true
          const resolve = () => void instance.recordsValue(source.uri).then((v) => live && emit(v ?? null), () => live && emit(null))
          resolve()
          const stop = instance.subscribe(resolve)
          return () => {
            live = false
            stop()
          }
        },
      },
      navigate: ({ screen, params }) => void this.navigate({ screen, params }),
      onSend: (message) => this.onSend(message),
      onActionFailed: ({ message }) => this.toast(message),
    })
  }

  // ------------------------------------------------------------ start

  /** Read the bundle, load the ONE instance, launch it (a deep link = its cold launch). */
  async start(link: LifecycleEvent | null = null): Promise<void> {
    const rawConfig = await this.options.readBundle(RUNNER_BUNDLE.files.config)
    let parsedConfig: unknown = null
    try {
      parsedConfig = rawConfig === null ? null : JSON.parse(rawConfig)
    } catch {
      parsedConfig = null
    }
    const { config } = runnerConfig(parsedConfig)
    if (!config) return this.fail(`config`)
    this.config = config
    const rawPackage = await this.options.readBundle(RUNNER_BUNDLE.files.package)
    if (rawPackage === null) return this.fail(`missing`)
    let file: unknown
    try {
      file = JSON.parse(rawPackage)
    } catch {
      return this.fail(`invalid`)
    }
    const pkg = packageOf(file)
    const appId = isObject(pkg) && typeof pkg.id === `string` ? pkg.id : config.id
    const storage = await this.options.openStorage(appId)
    const deviceId = await localDeviceId(storage.vault, runnerFixture.storage.deviceKey)
    try {
      this.instance = await AppInstance.load(
        {
          host: { kind: `exported`, platform: `web`, formFactor: this.formFactor === `wide` ? `desktop` : `phone`, locale: this.options.locale ?? `en` },
          storage: storage.records,
          blobs: storage.blobs,
          identity: { kind: `local`, deviceId },
          vault: storage.vault,
          window: { open: (_app, screen, params) => this.open(screen, params) },
          network: config.network,
          sync: null,
          capabilities: this.options.capabilities,
          newId: this.ids,
        },
        file,
      )
    } catch (error) {
      return this.fail(error instanceof LoadError ? error.kind : `invalid`)
    }
    this.install()
    await this.event(link ?? { type: `launch` })
    // A plain launch that opens nothing (a package without screens): the derived package's first screen.
    if (!this.instance.state.screen) {
      const first = (runnerPackage(this.instance.pkg, this.formFactor).screens as { template: string }[] | undefined)?.[0]
      if (first) this.open(first.template, {})
    }
  }

  private fail(kind: RunnerFailure): void {
    this.failure = kind
    this.paint(`app`, runnerFailure(kind))
  }

  private install(): void {
    this.host.installPackage(runnerPackage(this.instance!.pkg, this.formFactor) as unknown as AppPackage)
  }

  // ------------------------------------------------------------ lifecycle

  /** One page event (launch, visibility, pagehide, online, a link or a notification tap). */
  async event(event: LifecycleEvent): Promise<void> {
    if (!this.instance) return
    for (const effect of await this.instance.event(event)) if (effect.effect === `refused`) this.toast(lifecycle.copy.refused)
  }

  /** The window crossed `md`: re-install the shell for the form factor and re-apply the screen. */
  setFormFactor(formFactor: FormFactor): void {
    if (formFactor === this.formFactor) return
    this.formFactor = formFactor
    if (!this.instance) return
    this.install()
    const { screen, params } = this.instance.state
    if (screen) this.open(screen, params ?? {})
  }

  /** The WindowHost's `open`: the screen's template onto surface `app` with `{params}`. */
  private open(screen: string, params: Record<string, string>): void {
    this.host.receive({ version: VERSION, applyTemplate: { surfaceId: `app`, templateId: screen, packageId: this.instance!.appId, data: { params } } } as ServerMessage)
  }

  private async navigate(args: unknown): Promise<void> {
    const result = await this.instance?.call(`app.navigate`, args)
    if (result && `refused` in result) this.toast(result.refused.message)
  }

  // ------------------------------------------------------------ calls

  /** Every function the app may name goes to the instance (the package's `functions` narrow it). */
  private functions(): Record<string, HostFunction> {
    const out: Record<string, HostFunction> = {}
    for (const name of [...RECORDS_FUNCTIONS, ...CAPABILITY_TOOLS.map((t) => t.name)]) out[name] = (args) => this.call(name, args)
    return out
  }

  private call(name: string, args: unknown): Promise<unknown> {
    const instance = this.instance
    if (!instance) return Promise.reject(new Error(`no app`))
    const ids = Array.from({ length: createsOf(name, args) }, () => this.ids())
    return instance.call(name, args, ids).then((result) => this.settle(name, args, result))
  }

  /** A call's answer: its value, a refusal's toast (the call rejects), or the consent sheet (it waits for the answer). */
  private settle(name: string, args: unknown, result: CallResult): Promise<unknown> {
    if (`value` in result) return Promise.resolve(result.value)
    if (`refused` in result) {
      this.toast(result.refused.message)
      return Promise.reject(new Error(result.refused.message))
    }
    // One sheet at a time: a second ask while one shows is dismissed.
    this.pending?.settle(Promise.reject(new Error(`consentDismissed`)))
    return new Promise((resolve, reject) => {
      this.pending = { name, args, ask: result.ask, settle: (p) => p.then(resolve, reject) }
      this.paint(`consent`, result.sheet.components, result.sheet.data)
    })
  }

  private answer(answer: ConsentAnswer): void {
    const pending = this.pending
    if (!pending || !this.instance) return
    this.pending = null
    this.close(`consent`)
    // Inside the press's task: the native picker keeps the user activation.
    const result = this.instance.answerConsent(pending.name, pending.args, pending.ask, answer).then((r) => this.settle(pending.name, pending.args, r))
    pending.settle(result)
  }

  // ------------------------------------------------------------ events

  private onSend(message: ClientMessage): void {
    if (!(`action` in message)) return
    const { name, context } = message.action
    if (name === `runner.navigate`) void this.navigate({ screen: context.screen })
    else if (name === `runner.permissions`) this.paintPermissions()
    else if (name === `consent.toggle` && typeof context.scope === `string`) this.toggle(context.scope)
    else if (name === `consent.answer`) this.answer(([`allow`, `once`, `deny`, `dismiss`] as const).find((a) => a === context.answer) ?? `dismiss`)
  }

  /** `fresh` = open it anew (its Drawer closes itself); else repaint in place (a toggle). */
  private paintPermissions(fresh = true): void {
    if (!this.instance) return
    if (fresh || !this.host.surface(`permissions`)) this.paint(`permissions`, this.instance.permissionsSheet())
    else this.host.receive({ version: VERSION, updateComponents: { surfaceId: `permissions`, components: this.instance.permissionsSheet() } } as ServerMessage)
  }

  /** The Switch's gesture: granted → revoke, else granted. */
  private toggle(scope: string): void {
    if (!this.instance) return
    const refused = this.instance.setGrant(`consentUi`, true, scope, this.instance.grants()[scope] === `granted` ? null : `granted`)
    if (refused) this.toast(refused)
    this.paintPermissions(false)
  }

  // ------------------------------------------------------------ surfaces

  private paint(surfaceId: string, components: SurfaceComponent[], data?: Record<string, unknown>): void {
    if (this.host.surface(surfaceId)) this.host.receive({ version: VERSION, deleteSurface: { surfaceId } } as ServerMessage)
    this.host.receive({ version: VERSION, createSurface: { surfaceId, catalogId: CORE_CATALOG_ID } } as ServerMessage)
    this.host.receive({ version: VERSION, updateComponents: { surfaceId, components } } as ServerMessage)
    if (data) this.host.receive({ version: VERSION, updateDataModel: { surfaceId, path: `/`, value: data } } as ServerMessage)
  }

  private close(surfaceId: string): void {
    if (this.host.surface(surfaceId)) this.host.receive({ version: VERSION, deleteSurface: { surfaceId } } as ServerMessage)
  }

  /** The failed-action toast; it dismisses itself. */
  toast(message: string): void {
    this.paint(`toast`, runnerToast(message))
    const clear = this.options.clearTimeout ?? ((t) => clearTimeout(t as ReturnType<typeof setTimeout>))
    const set = this.options.setTimeout ?? ((fn, ms) => setTimeout(fn, ms))
    if (this.toastTimer !== null) clear(this.toastTimer)
    this.toastTimer = set(() => {
      this.toastTimer = null
      this.close(`toast`)
    }, TOAST_MS)
  }
}
