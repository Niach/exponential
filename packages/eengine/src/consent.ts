// VAPP-260: consent. The answer to the sheet, who may write a grant, the
// Vault record and its reconcile on every package load, and the two genUI
// surfaces (the ask sheet, the Permissions sheet) built from existing
// components only. Locked by `fixtures/consent.json`.

import { canonicalJson } from "@exp/emcp"

import fixture from "../fixtures/consent.json" with { type: "json" }
import { type BrokerVerdictKind, type Grants, type GrantValue, type ManifestEntry, scopeDef } from "./capabilities"

export type ConsentAnswer = `allow` | `once` | `deny` | `dismiss`
export type GrantVia = `consentUi` | `agent` | `hubTool` | `package` | `hostConfig` | `sync`

export const CONSENT_COPY = fixture.copy
export const GRANTS_RECORD_VERSION: number = fixture.recordVersion

/** A genUI component (the flat A2UI list a surface carries). */
export type SurfaceComponent = Record<string, unknown> & { id: string; component: string }

const sortGrants = (grants: Grants): Grants => Object.fromEntries(Object.entries(grants).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))) as Grants

/** The sheet's answer → the call's verdict and the grants to store. */
export function consentAnswer(grants: Grants, scope: string, answer: ConsentAnswer): { verdict: BrokerVerdictKind; reason: string | null; grants: Grants } {
  if (answer === `allow`) return { verdict: `call`, reason: null, grants: sortGrants({ ...grants, [scope]: `granted` }) }
  if (answer === `once`) return { verdict: `call`, reason: null, grants: sortGrants({ ...grants }) }
  if (answer === `deny`) return { verdict: `denied`, reason: `consentDenied`, grants: sortGrants({ ...grants, [scope]: `denied` }) }
  return { verdict: `denied`, reason: `consentDismissed`, grants: sortGrants({ ...grants }) }
}

/** Is `id` a declared `ask` scope of the manifest (one a grant is kept for)? */
function declaredAsk(manifest: readonly ManifestEntry[], id: string): boolean {
  const def = scopeDef(id)
  return !!def && !def.scope.hostOnly && def.scope.consent === `ask` && manifest.some((m) => m.id === id)
}

/** A grant write: only the ConsentUi, only after a gesture, only an ask scope. */
export function grantWrite(
  manifest: readonly ManifestEntry[],
  grants: Grants,
  write: { via: GrantVia; gesture: boolean; scope: string; value: GrantValue | null },
): { grants: Grants; refused: string | null } {
  if (write.via !== `consentUi`) return { grants, refused: `onlyConsentUi` }
  if (!write.gesture) return { grants, refused: `noGesture` }
  if (!declaredAsk(manifest, write.scope)) return { grants, refused: `notAsk` }
  const next = { ...grants }
  if (write.value === null) delete next[write.scope]
  else next[write.scope] = write.value
  return { grants: sortGrants(next), refused: null }
}

const httpOrigins = (manifest: readonly ManifestEntry[]): string[] => [...(manifest.find((m) => m.id === `http.fetch`)?.origins ?? [])].sort()

/** The Vault key and canonical JSON value of an app's grants. */
export function grantsRecord(app: string, signer: string, manifest: readonly ManifestEntry[], grants: Grants): { key: string; value: string } {
  return {
    key: fixture.vaultKey.replace(`{app}`, app),
    value: canonicalJson({ grants: sortGrants(grants), origins: httpOrigins(manifest), signer, v: GRANTS_RECORD_VERSION }),
  }
}

/** The grants that survive a package load (`stored` = the Vault value, or null). */
export function grantsOnPackage(stored: string | null, signer: string, manifest: readonly ManifestEntry[]): Grants {
  if (stored === null) return {}
  let record: unknown
  try {
    record = JSON.parse(stored)
  } catch {
    return {}
  }
  if (!record || typeof record !== `object`) return {}
  const r = record as { v?: unknown; signer?: unknown; grants?: unknown; origins?: unknown }
  if (r.v !== GRANTS_RECORD_VERSION || r.signer !== signer || !r.grants || typeof r.grants !== `object`) return {}
  const granted = Array.isArray(r.origins) ? r.origins.filter((o): o is string => typeof o === `string`) : []
  const out: Grants = {}
  for (const [id, value] of Object.entries(r.grants as Record<string, unknown>)) {
    if (value !== `granted` && value !== `denied`) continue
    if (!declaredAsk(manifest, id)) continue
    if (scopeDef(id)?.scope.origins && !httpOrigins(manifest).every((o) => granted.includes(o))) continue
    out[id] = value
  }
  return sortGrants(out)
}

/** The ask sheet for one manifest entry: the surface's components and data. */
export function consentSheet(app: string, entry: ManifestEntry): { components: SurfaceComponent[]; data: { answer: ConsentAnswer } } {
  const def = scopeDef(entry.id)
  const verb = (def?.scope.verb ?? entry.id).replace(`{origins}`, (entry.origins ?? []).join(`, `))
  const title = fixture.copy.title.replace(`{app}`, app).replace(`{verb}`, verb)
  return {
    components: [
      {
        id: `root`,
        component: `Drawer`,
        title,
        open: true,
        side: `bottom`,
        children: [`why`, `answer`],
        slots: { footer: `confirm` },
        on: { change: { event: { name: `consent.answer`, context: { answer: `dismiss` } } } },
      },
      { id: `why`, component: `Text`, text: entry.why },
      { id: `answer`, component: `ChoiceTiles`, name: `answer`, columns: 1, value: { "@path": `/answer` }, options: fixture.copy.options.map((o) => ({ ...o })) },
      { id: `confirm`, component: `Button`, label: fixture.copy.confirm, on: { press: { event: { name: `consent.answer`, context: { answer: { "@path": `/answer` } } } } } },
    ],
    data: { answer: `allow` },
  }
}

/** The Permissions sheet: one Switch per declared ask scope, in manifest order. */
export function manageSheet(manifest: readonly ManifestEntry[], grants: Grants): SurfaceComponent[] {
  const rows = manifest.filter((m) => declaredAsk(manifest, m.id))
  return [
    { id: `root`, component: `Drawer`, title: fixture.copy.manageTitle, open: true, side: `bottom`, children: rows.map((_, i) => `scope_${i}`) },
    ...rows.map((m, i) => ({
      id: `scope_${i}`,
      component: `Switch`,
      label: scopeDef(m.id)!.capability.label,
      description: m.why,
      checked: grants[m.id] === `granted`,
      on: { change: { event: { name: `consent.toggle`, context: { scope: m.id } } } },
    })),
  ]
}
