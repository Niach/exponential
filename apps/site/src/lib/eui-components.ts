/* genUI's components as the site lists them: one page per catalog component at
   /docs/sdk/genui/reference/components/<slug>/ (slug = specimen id minus
   the prefix), grouped in this order. Shared by the sidebar, the generator
   (scripts/reference/eui-components.ts) and the index. */
export const EUI_COMPONENTS_BASE = `/docs/sdk/genui/reference/components`

export const EUI_GROUP_ORDER = [`layout`, `text`, `media`, `navigation`, `overlay`, `feedback`, `data`, `action`, `input`, `list`] as const

export const EUI_GROUP_LABELS: Record<string, string> = {
  layout: `Layout`,
  text: `Text`,
  media: `Media`,
  navigation: `Navigation`,
  overlay: `Overlays`,
  feedback: `Feedback`,
  data: `Data`,
  action: `Actions`,
  input: `Inputs`,
  list: `Lists`,
}

export const euiComponentSlug = (c: { specimenId: string }) => c.specimenId.replace(/^exponential-ui-/, ``)
export const euiComponentPath = (c: { specimenId: string }) => `${EUI_COMPONENTS_BASE}/${euiComponentSlug(c)}/`
