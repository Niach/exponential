import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { App } from "./app"
import { parseView } from "./model"
import "./styles.css"

// The server stamps the view into the one built document (apps/web/src/lib/
// mcp/apps.ts): `<meta name="exp-view" content="issues|issue|run">`.
const view = parseView(
  document.querySelector(`meta[name="exp-view"]`)?.getAttribute(`content`)
)

const root = document.getElementById(`root`)
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App view={view} />
    </StrictMode>
  )
}
