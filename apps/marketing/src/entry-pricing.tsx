import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { PricingPage } from "./PricingPage"
import "./styles.css"

const container = document.getElementById(`root`)
if (!container) throw new Error(`root not found`)
createRoot(container).render(
  <StrictMode>
    <PricingPage />
  </StrictMode>
)
