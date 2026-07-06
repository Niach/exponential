import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { DownloadPage } from "./DownloadPage"
import "./styles.css"

const container = document.getElementById(`root`)
if (!container) throw new Error(`root not found`)
createRoot(container).render(
  <StrictMode>
    <DownloadPage />
  </StrictMode>
)
