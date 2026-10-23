/* "From the blog" on the home page: the newest post as one wide card. */
import { motion } from "motion/react"
import { sectionReveal } from "../../lib/animations"
import { BLOG_POSTS } from "../../blog/posts"
import { BLOG_PATH } from "../../blog/types"
import { BlogCard } from "./BlogCard"

export function BlogStrip() {
  const newest = BLOG_POSTS[0]
  if (!newest) return null
  return (
    <section id="blog" className="home-blog">
      <div className="shell">
        <motion.div {...sectionReveal}>
          <h2 className="section-title">From the blog</h2>
          <p className="section-sub">
            Where the product stands and where it goes, with the screenshots
            and the issue numbers behind every claim.
          </p>
        </motion.div>
        <BlogCard meta={newest.meta} strip />
        <p className="blog-strip-all">
          <a href={BLOG_PATH}>All posts</a>
        </p>
      </div>
    </section>
  )
}
