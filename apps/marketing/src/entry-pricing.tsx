import { StrictMode } from "react"
import { createRoot, hydrateRoot } from "react-dom/client"
import { PricingPage } from "./PricingPage"
import "./styles.css"

const container = document.getElementById(`root`)
if (!container) throw new Error(`root not found`)

const app = (
  <StrictMode>
    <PricingPage />
  </StrictMode>
)

// Prod HTML is prerendered (scripts/prerender.tsx) — hydrate it; dev is empty — mount fresh.
if (container.firstChild) {
  hydrateRoot(container, app)
} else {
  createRoot(container).render(app)
}
