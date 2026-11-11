import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { GenuiApp } from "./genui-app"
import "../styles.css"

// #VAPP-292: the genUI view's document (`genui.html`, vite.config.ts).
const root = document.getElementById(`root`)
if (root) {
  createRoot(root).render(
    <StrictMode>
      <GenuiApp />
    </StrictMode>
  )
}
