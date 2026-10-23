import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { BlogCard } from "./components/blog/BlogCard"
import { BLOG_POSTS } from "./blog/posts"
import { BLOG_PATH } from "./blog/types"

export function BlogIndexPage() {
  return (
    <>
      <SiteHeader currentPath={BLOG_PATH} />

      <main>
        <section className="docs-hero">
          <div className="shell docs-hero-content">
            <h1>Blog</h1>
            <p>
              Where the product stands and where it goes, with the screenshots
              and the issue numbers behind every claim.
            </p>
            <div className="docs-hero-cta">
              <a className="btn btn-ghost btn-sm" href={`${BLOG_PATH}rss.xml`}>
                RSS feed
              </a>
            </div>
          </div>
        </section>

        <section className="blog-index">
          <div className="shell">
            <div className="blog-list">
              {BLOG_POSTS.map((post, i) => (
                <BlogCard key={post.meta.slug} meta={post.meta} priority={i === 0} />
              ))}
            </div>
          </div>
        </section>
      </main>

      <SiteFooter />
    </>
  )
}
