import { defineConfig } from "vite"
import solid from "vite-plugin-solid"

// Solid's JSX needs its compiler.
export default defineConfig({ plugins: [solid()] })
