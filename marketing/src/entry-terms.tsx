import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { TermsPage } from "./TermsPage"
import "./styles.css"

const container = document.getElementById(`root`)
if (!container) throw new Error(`root not found`)
createRoot(container).render(
  <StrictMode>
    <TermsPage />
  </StrictMode>
)
