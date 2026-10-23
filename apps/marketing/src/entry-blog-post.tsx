/* ONE entry for every post page: the slug comes from the URL
   (/blog/<slug>/), the post from the registry, so a new post needs no new
   entry module. The prerender renders the same component for the same path. */
import { StrictMode } from "react"
import { createRoot, hydrateRoot } from "react-dom/client"
import { BlogPostPage } from "./BlogPostPage"
import { postBySlug } from "./blog/posts"
import "./styles.css"

const container = document.getElementById(`root`)
if (!container) throw new Error(`root not found`)

const slug = window.location.pathname.replace(/^\/blog\//, ``).replace(/\/.*$/, ``)
const post = postBySlug(slug)
if (!post) throw new Error(`blog: no post for slug "${slug}"`)

const app = (
  <StrictMode>
    <BlogPostPage post={post} />
  </StrictMode>
)

// Prod HTML is prerendered (scripts/prerender.tsx) — hydrate it; dev is empty — mount fresh.
if (container.firstChild) {
  hydrateRoot(container, app)
} else {
  createRoot(container).render(app)
}
