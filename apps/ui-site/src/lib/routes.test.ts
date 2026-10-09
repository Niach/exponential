import { expect, test } from "bun:test"
import { COMPONENT_DOCS } from "./catalog"
import { GUIDES } from "./guides"
import { pageLoaderFor } from "./pages"
import { ROUTES } from "./routes"

test(`every manifest route has a page loader and vice versa`, async () => {
  for (const route of ROUTES) {
    const load = pageLoaderFor(route.path)
    expect(load, route.path).toBeDefined()
    expect(await load!()).toBe(await route.load())
  }
})

test(`one page per component and per guide, paths unique`, () => {
  const paths = ROUTES.map((r) => r.path)
  expect(new Set(paths).size).toBe(paths.length)
  expect(paths.filter((p) => /^\/components\/.+\//.test(p)).length).toBe(COMPONENT_DOCS.length)
  expect(paths.filter((p) => /^\/guides\/.+\//.test(p)).length).toBe(GUIDES.length)
})

test(`titles and descriptions fit the search snippet`, () => {
  for (const route of ROUTES) {
    expect(route.title.length, route.path).toBeLessThanOrEqual(80)
    expect(route.description.length, route.path).toBeGreaterThan(40)
    expect(route.description.length, route.path).toBeLessThanOrEqual(220)
  }
})
