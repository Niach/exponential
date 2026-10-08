/* The few catalog facts the content pages print (counts, ids, component
   links) WITHOUT importing the generated component docs: those weigh ~140 KB
   and belong to the component pages only. catalog-facts.test.ts locks every
   value here to the docs, so a catalog change fails the test, not the page. */
import promptBudget from "@exponential-at/ui/fixtures/prompt-budget.json"

export const CORE_CATALOG = `https://ui.exponential.at/catalogs/core/v1`
export const CORE_LITE_CATALOG = `https://ui.exponential.at/catalogs/core-lite/v1`

/** Every component of the core catalog (the prompt's count, generated). */
export const COMPONENT_COUNT: number = promptBudget.full.components
export const NATIVE_COUNT = 45
export const MACRO_COUNT = 36

/** A component's page: its slug is its name in kebab case (`DropdownMenu` → `dropdown-menu`). */
export const componentSlugOf = (name: string) => name.replace(/([a-z0-9])([A-Z])/g, `$1-$2`).toLowerCase()
export const componentHref = (name: string) => `/components/${componentSlugOf(name)}/`
