import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { PrivacyPage } from "./PrivacyPage"
import "./styles.css"

const container = document.getElementById(`root`)
if (!container) throw new Error(`root not found`)
createRoot(container).render(
  <StrictMode>
    <PrivacyPage />
  </StrictMode>
)
