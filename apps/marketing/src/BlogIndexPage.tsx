import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { PostList } from "./components/blog/PostList"
import { BLOG_POSTS } from "./blog/posts"
import { BLOG_PATH } from "./blog/types"

export function BlogIndexPage() {
  return (
    <>
      <SiteHeader currentPath={BLOG_PATH} />

      <main>
        <section className="blog-index">
          <div className="shell blog-index-inner">
            <h1>Blog</h1>
            <p className="blog-index-intro">
              Notes on how Exponential works and where it goes, with a screenshot behind each claim.{` `}
              <a href={`${BLOG_PATH}rss.xml`}>RSS</a>
            </p>
            <PostList posts={BLOG_POSTS.map((post) => post.meta)} byYear />
          </div>
        </section>
      </main>

      <SiteFooter />
    </>
  )
}
