/* The site's data: the generated catalog docs, theme docs and specimens of
   @exponential-at/ui (docs/*.generated.json, fixtures/specimens.json). One
   page per component; its slug is its specimen id minus the prefix. */
import componentsDoc from "@exponential-at/ui/docs/components.generated.json"
import themesDoc from "@exponential-at/ui/docs/themes.generated.json"
import specimensDoc from "@exponential-at/ui/fixtures/specimens.json"

export type ComponentDoc = (typeof componentsDoc.components)[number]
export type ThemesDoc = typeof themesDoc
export type Specimen = (typeof specimensDoc.specimens)[number]

export const COMPONENT_DOCS: readonly ComponentDoc[] = componentsDoc.components
export const COMPONENT_GROUPS: readonly string[] = componentsDoc.groups
export const CATALOG_ID: string = componentsDoc.catalogId
export const LITE_CATALOG_ID: string = componentsDoc.liteCatalogId
export const THEMES_DOC: ThemesDoc = themesDoc
export const SPECIMENS: readonly Specimen[] = specimensDoc.specimens

const PREFIX = `exponential-ui-`

export const componentSlug = (doc: ComponentDoc) => doc.specimenId.slice(PREFIX.length)
export const componentPath = (doc: ComponentDoc) => `/components/${componentSlug(doc)}/`
export const componentBySlug = (slug: string) => COMPONENT_DOCS.find((d) => componentSlug(d) === slug)
export const specimenById = (id: string) => SPECIMENS.find((s) => s.id === id)

/** The four platforms every specimen is photographed on, in page order. */
export const SHOT_PLATFORMS = [
  { id: `web`, label: `Web`, renderer: `React` },
  { id: `ios`, label: `iOS`, renderer: `SwiftUI` },
  { id: `android`, label: `Android`, renderer: `Compose` },
  { id: `desktop`, label: `Desktop`, renderer: `gpui` },
] as const

/** The stored shot of a view on a platform (the build copies shots/ into dist). */
export const shotUrl = (viewId: string, platform: string) => `/shots/${viewId}/${platform}.webp`
