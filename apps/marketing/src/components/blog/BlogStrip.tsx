/* "From the blog" on the home page: the newest posts as rows. */
import { BLOG_POSTS } from "../../blog/posts"
import { BLOG_PATH } from "../../blog/types"
import { PostList } from "./PostList"

export function BlogStrip() {
  if (BLOG_POSTS.length === 0) return null
  return (
    <section id="blog" className="home-blog">
      <div className="shell blog-strip-inner">
        <h2 className="section-title">From the blog</h2>
        <PostList posts={BLOG_POSTS.slice(0, 3).map((post) => post.meta)} />
        <p className="blog-strip-all">
          <a href={BLOG_PATH}>All posts</a>
        </p>
      </div>
    </section>
  )
}
