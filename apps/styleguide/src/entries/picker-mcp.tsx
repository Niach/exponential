import {
  McpServerPicker,
  PickerList,
  PickerTrigger,
  mcpServerPickerItems,
} from "@exp/ui"

import { PICKER_FILES, PickerSpecimen } from "./picker-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

const noop = (): void => {}

/* A server whose host is in the MCP catalog wears the REAL brand mark of
   its service (the generated selfh.st light SVG), a command server the
   terminal, anything else the plug. A server the member has not connected
   yet still renders, its reason as the muted second line. */
const SERVERS = [
  { id: `linear`, name: `linear`, url: `https://mcp.linear.app/mcp` },
  {
    id: `sentry`,
    name: `sentry`,
    url: `https://mcp.sentry.dev/mcp`,
    description: `Connect first`,
  },
  { id: `docs`, name: `acme-docs`, url: `https://docs.acme.dev/mcp` },
  { id: `local`, name: `browser`, command: `npx @playwright/mcp` },
]

export const entry: StyleguideEntry = {
  id: `picker-mcp`,
  section: `general`,
  owner: `EXP-792`,
  title: `MCP server picker`,
  blurb: `The team's MCP servers a run connects to — the Agent composer's "MCP servers" row (multi) and, as the same rows in a PickerList, the Add dialog's one search box over the well-known servers (type a name, or paste any URL and pick its "Use …" row). A server whose host is in the catalog (contract mcpCatalog) wears the REAL brand mark of its service, one of the vendored selfh.st light SVGs in packages/icons brand, reproduced as-is and never recoloured; a command server the terminal, anything else the plug. One the member has not connected renders with "Connect first" as its second line rather than vanishing.`,
  status: {
    web: {
      state: `ok`,
      symbol: `McpServerPicker`,
      file: `${PICKER_FILES.web}/mcp-server-picker.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `picker::mcp_server_picker`,
      file: `${PICKER_FILES.desktop}/mcp_server_picker.rs`,
    },
    ios: { state: `n/a`, note: `The native composer has no MCP row yet; its runs connect none.` },
    android: { state: `n/a`, note: `The native composer has no MCP row yet; its runs connect none.` },
  },
  island: () => (
    <PickerSpecimen
      trigger={
        <McpServerPicker
          servers={SERVERS}
          value={[`linear`, `docs`]}
          onChange={noop}
          trigger={
            <PickerTrigger variant="row" label="MCP servers" value="linear, acme-docs" />
          }
        />
      }
      surface={
        <PickerList
          mode="multi"
          items={mcpServerPickerItems(SERVERS)}
          value={[`linear`, `docs`]}
          onChange={noop}
          search
          searchPlaceholder="Search servers…"
        />
      }
    />
  ),
}
