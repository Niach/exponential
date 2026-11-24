import { createContext, useContext } from "react"

// #NEW-24: the colour mode the genUI pieces of this package paint in (the
// Markdown scope, a card's surface). The app is dark on every client, so
// dark is the default and the app never provides another; the MCP Apps
// views provide the HOST's mode (packages/mcp-apps), whose light is the
// `exponential` theme's own light mode.
export type ColorMode = `light` | `dark`

const ColorModeContext = createContext<ColorMode>(`dark`)

export const ColorModeProvider = ColorModeContext.Provider

export function useColorMode(): ColorMode {
  return useContext(ColorModeContext)
}
