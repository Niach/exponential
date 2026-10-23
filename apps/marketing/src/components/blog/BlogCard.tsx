/* One post as a card: the index grid, a post's "More from the blog" and
   the home strip share it. */
import { formatDate, postPath, type PostMeta } from "../../blog/types"
import { IcArrow } from "../icons"

export function BlogCard({ meta, priority = false, strip = false }: { meta: PostMeta; priority?: boolean; strip?: boolean }) {
  return (
    <a className={`glass-card ${strip ? `blog-strip-card` : `blog-card`}`} href={postPath(meta.slug)}>
      <img
        className="blog-card-cover"
        src={meta.cover.src}
        alt={meta.cover.alt}
        width={meta.cover.width}
        height={meta.cover.height}
        loading={priority ? `eager` : `lazy`}
        decoding="async"
      />
      <div className="blog-card-body">
        <span className="blog-card-meta">
          <time dateTime={meta.date}>{formatDate(meta.date)}</time> · {meta.readingMinutes} min read
        </span>
        <h2 className="blog-card-title">{meta.title}</h2>
        <p className="blog-card-text">{meta.description}</p>
        <span className="blog-card-more">
          Read the post <IcArrow size={12} />
        </span>
      </div>
    </a>
  )
}
