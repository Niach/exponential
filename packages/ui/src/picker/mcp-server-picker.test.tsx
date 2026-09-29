import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { contract } from "@exp/domain-contract"
import { BRAND_ICONS } from "../brand-icons.generated"
import { conceptIcon } from "../icons.generated"
import {
  MCP_CATALOG,
  getMcpServerIcon,
  isMcpCatalogTemplate,
  mcpCatalogEntryFor,
  mcpServerPickerItems,
} from "./mcp-server-picker"
import { PickerItemBody } from "./picker"

// EXP-792 — a server's glyph is the REAL brand mark of the service its URL
// belongs to (contract `mcpCatalog` → packages/icons `brand`), a terminal
// for a command, the plug for anything else. The marks keep their own white
// fills; the row's tint never recolours them.
describe(`MCP server picker rows`, () => {
  it(`reads the catalog off the contract, typed, templates flagged`, () => {
    expect(MCP_CATALOG.map((e) => e.id)).toEqual(
      contract.mcpCatalog.servers.map((s) => s.id)
    )
    const selfManaged = MCP_CATALOG.find((e) => e.id === `gitlab-self-managed`)!
    expect(selfManaged.template).toBe(true)
    expect(selfManaged.url).toBe(`https://{host}/api/v4/mcp`)
    expect(isMcpCatalogTemplate(`https://gitlab.com/api/v4/mcp`)).toBe(false)
    expect(MCP_CATALOG.filter((e) => !e.template).every((e) => !e.url.includes(`{`))).toBe(true)
  })

  it(`matches a hosted entry by host, a template by its path on any host`, () => {
    expect(mcpCatalogEntryFor(`https://mcp.linear.app/mcp`)?.id).toBe(`linear`)
    expect(mcpCatalogEntryFor(`HTTPS://MCP.LINEAR.APP/mcp?x=1`)?.id).toBe(`linear`)
    expect(mcpCatalogEntryFor(`https://gitlab.com/api/v4/mcp`)?.id).toBe(`gitlab`)
    expect(mcpCatalogEntryFor(`https://gitlab.acme.dev/api/v4/mcp`)?.id).toBe(
      `gitlab-self-managed`
    )
    expect(mcpCatalogEntryFor(`https://docs.acme.dev/mcp`)).toBeUndefined()
    expect(mcpCatalogEntryFor(`not a url`)).toBeUndefined()
    expect(mcpCatalogEntryFor(null)).toBeUndefined()
  })

  it(`hands a catalog host its brand mark, a command the terminal, the rest the plug`, () => {
    expect(getMcpServerIcon({ url: `https://mcp.sentry.dev/mcp` })).toBe(BRAND_ICONS.sentry)
    expect(getMcpServerIcon({ url: `https://gitlab.acme.dev/api/v4/mcp` })).toBe(
      BRAND_ICONS.gitlab
    )
    expect(getMcpServerIcon({ command: `npx @playwright/mcp` })).toBe(
      conceptIcon(`session-shell`)
    )
    expect(getMcpServerIcon({ url: `https://docs.acme.dev/mcp` })).toBe(conceptIcon(`ui-mcp`))
    // A command WITH a URL is an http server that happens to name a command.
    expect(getMcpServerIcon({ url: `https://docs.acme.dev/mcp`, command: `x` })).toBe(
      conceptIcon(`ui-mcp`)
    )
  })

  it(`draws a brand mark in the glyph slot at glyph size, fills untouched`, () => {
    const [item] = mcpServerPickerItems([
      { id: `s`, name: `Stripe`, url: `https://mcp.stripe.com` },
    ])
    const { container } = render(<PickerItemBody item={{ ...item!, color: `text-red-500` }} />)
    const svg = container.querySelector(`svg`)!
    expect(svg.getAttribute(`class`)).toContain(`size-4`)
    expect(svg.getAttribute(`class`)).toContain(`shrink-0`)
    expect(svg.getAttribute(`class`)).toContain(`text-red-500`)
    expect(svg.getAttribute(`viewBox`)).toBe(`0 0 512 512`)
    expect(svg.getAttribute(`aria-hidden`)).toBe(`true`)
    // The mark's own fill, not the tint.
    expect(container.querySelector(`path`)?.getAttribute(`fill`)).toBe(`#fff`)
    expect(container.innerHTML).not.toContain(`currentColor`)
    // A concept glyph in the same slot, same size.
    const [plug] = mcpServerPickerItems([{ id: `d`, name: `docs`, url: `https://docs.acme.dev/mcp` }])
    const concept = render(<PickerItemBody item={plug!} />).container.querySelector(`svg`)!
    expect(concept.getAttribute(`class`)).toContain(`size-4`)
    expect(concept.getAttribute(`viewBox`)).toBe(`0 0 24 24`)
  })

  it(`seeds the second line and the keywords from the host or the command`, () => {
    const items = mcpServerPickerItems([
      { id: `a`, name: `linear`, url: `https://mcp.linear.app/mcp` },
      { id: `b`, name: `browser`, command: `npx @playwright/mcp` },
      { id: `c`, name: `sentry`, url: `https://mcp.sentry.dev/mcp`, description: `Connect first` },
    ])
    expect(items[0]!.description).toBe(`mcp.linear.app`)
    expect(items[0]!.keywords).toEqual([`linear`, `mcp.linear.app`])
    expect(items[1]!.description).toBe(`npx @playwright/mcp`)
    expect(items[2]!.description).toBe(`Connect first`)
  })
})
