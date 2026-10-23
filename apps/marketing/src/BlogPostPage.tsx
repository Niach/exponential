import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { PostList } from "./components/blog/PostList"
import { Figure } from "./components/blog/Figure"
import { BLOG_POSTS, type BlogPost } from "./blog/posts"
import { BLOG_PATH, formatDate } from "./blog/types"

export function BlogPostPage({ post }: { post: BlogPost }) {
  const { meta, Post } = post
  const others = BLOG_POSTS.filter((p) => p.meta.slug !== meta.slug).slice(0, 2)
  return (
    <>
      <SiteHeader currentPath={BLOG_PATH} />

      <main>
        <article className="shell blog-post">
          <header className="blog-post-head">
            <p className="blog-eyebrow">
              <a href={BLOG_PATH}>Blog</a> · <time dateTime={meta.date}>{formatDate(meta.date)}</time> · {meta.readingMinutes} min read
            </p>
            <h1>{meta.title}</h1>
            <p className="blog-lead">{meta.description}</p>
            <p className="blog-author">
              By {meta.author.url ? <a href={meta.author.url}>{meta.author.name}</a> : meta.author.name}
              {meta.updated ? <> · updated {formatDate(meta.updated)}</> : null}
            </p>
          </header>

          <Figure src={meta.cover.src} alt={meta.cover.alt} width={meta.cover.width} height={meta.cover.height} priority />

          <div className="docs-content blog-prose">
            <Post />
          </div>

          <footer className="blog-post-foot">
            {others.length > 0 ? (
              <>
                <h2>More from the blog</h2>
                <PostList posts={others.map((other) => other.meta)} />
              </>
            ) : null}
            <p className="blog-post-foot-links">
              <a href={BLOG_PATH}>All posts</a> · <a href={`${BLOG_PATH}rss.xml`}>RSS feed</a>
            </p>
          </footer>
        </article>
      </main>

      <SiteFooter />
    </>
  )
}
