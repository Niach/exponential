import * as React from "react"
import { ArrowLeftIcon, ArrowRightIcon } from "lucide-react"

import { cn } from "./cn"
import { Button } from "./button"

// VAPP-87: shadcn's carousel API (Carousel / CarouselContent / CarouselItem
// / CarouselPrevious / CarouselNext) on native CSS SCROLL SNAP instead of
// embla: the viewport is an overflow scroller with mandatory snap, each item
// a snap point, and the arrows `scrollBy` one viewport. Touch, trackpad and
// wheel scrolling are the browser's own; the arrows disable themselves at
// either end off the live scroll position.

type CarouselOrientation = `horizontal` | `vertical`

/** The imperative handle `setApi` receives once the viewport mounts. */
export interface CarouselApi {
  scrollPrev: () => void
  scrollNext: () => void
  /** Scroll item `index` (0-based) to the snap start. */
  scrollTo: (index: number) => void
  canScrollPrev: () => boolean
  canScrollNext: () => boolean
  viewport: HTMLDivElement
}

type CarouselProps = {
  orientation?: CarouselOrientation
  setApi?: (api: CarouselApi) => void
}

type CarouselContextProps = {
  viewportRef: React.RefObject<HTMLDivElement | null>
  orientation: CarouselOrientation
  scrollPrev: () => void
  scrollNext: () => void
  canScrollPrev: boolean
  canScrollNext: boolean
  onViewportScroll: () => void
}

const CarouselContext = React.createContext<CarouselContextProps | null>(null)

function useCarousel() {
  const context = React.useContext(CarouselContext)
  if (!context) {
    throw new Error(`useCarousel must be used within a <Carousel />`)
  }
  return context
}

/** A pixel of slack, so sub-pixel scroll positions still read as an end. */
const EDGE_EPSILON = 1

function Carousel({
  orientation = `horizontal`,
  setApi,
  className,
  children,
  ...props
}: React.ComponentProps<`div`> & CarouselProps) {
  const viewportRef = React.useRef<HTMLDivElement | null>(null)
  const [canScrollPrev, setCanScrollPrev] = React.useState(false)
  const [canScrollNext, setCanScrollNext] = React.useState(false)
  const horizontal = orientation === `horizontal`

  const readEdges = React.useCallback(() => {
    const el = viewportRef.current
    if (!el) return { prev: false, next: false }
    const position = horizontal ? el.scrollLeft : el.scrollTop
    const extent = horizontal
      ? el.scrollWidth - el.clientWidth
      : el.scrollHeight - el.clientHeight
    return {
      prev: position > EDGE_EPSILON,
      next: position < extent - EDGE_EPSILON,
    }
  }, [horizontal])

  const onViewportScroll = React.useCallback(() => {
    const edges = readEdges()
    setCanScrollPrev(edges.prev)
    setCanScrollNext(edges.next)
  }, [readEdges])

  const scrollByPage = React.useCallback(
    (direction: 1 | -1) => {
      const el = viewportRef.current
      if (!el) return
      const page = horizontal ? el.clientWidth : el.clientHeight
      el.scrollBy({
        [horizontal ? `left` : `top`]: direction * page,
        behavior: `smooth`,
      })
    },
    [horizontal]
  )
  const scrollPrev = React.useCallback(() => scrollByPage(-1), [scrollByPage])
  const scrollNext = React.useCallback(() => scrollByPage(1), [scrollByPage])

  const handleKeyDown = React.useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      const prevKey = horizontal ? `ArrowLeft` : `ArrowUp`
      const nextKey = horizontal ? `ArrowRight` : `ArrowDown`
      if (event.key === prevKey) {
        event.preventDefault()
        scrollPrev()
      } else if (event.key === nextKey) {
        event.preventDefault()
        scrollNext()
      }
    },
    [horizontal, scrollPrev, scrollNext]
  )

  React.useEffect(() => {
    const el = viewportRef.current
    if (!el) return
    onViewportScroll()
    if (typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(() => onViewportScroll())
    observer.observe(el)
    return () => observer.disconnect()
  }, [onViewportScroll])

  React.useEffect(() => {
    const el = viewportRef.current
    if (!el || !setApi) return
    setApi({
      scrollPrev,
      scrollNext,
      scrollTo: (index) => {
        const item = el.querySelectorAll<HTMLElement>(
          `[data-slot="carousel-item"]`
        )[index]
        if (!item) return
        el.scrollTo({
          [horizontal ? `left` : `top`]: horizontal
            ? item.offsetLeft - el.offsetLeft
            : item.offsetTop - el.offsetTop,
          behavior: `smooth`,
        })
      },
      canScrollPrev: () => readEdges().prev,
      canScrollNext: () => readEdges().next,
      viewport: el,
    })
  }, [setApi, scrollPrev, scrollNext, readEdges, horizontal])

  return (
    <CarouselContext.Provider
      value={{
        viewportRef,
        orientation,
        scrollPrev,
        scrollNext,
        canScrollPrev,
        canScrollNext,
        onViewportScroll,
      }}
    >
      <div
        onKeyDownCapture={handleKeyDown}
        className={cn(`relative`, className)}
        role="region"
        aria-roledescription="carousel"
        data-slot="carousel"
        data-orientation={orientation}
        {...props}
      >
        {children}
      </div>
    </CarouselContext.Provider>
  )
}

function CarouselContent({ className, ...props }: React.ComponentProps<`div`>) {
  const { viewportRef, orientation, onViewportScroll } = useCarousel()

  return (
    <div
      ref={viewportRef}
      onScroll={onViewportScroll}
      className={cn(
        `[scrollbar-width:none] [&::-webkit-scrollbar]:hidden`,
        orientation === `horizontal`
          ? `snap-x snap-mandatory overflow-x-auto overflow-y-hidden overscroll-x-contain`
          : `snap-y snap-mandatory overflow-x-hidden overflow-y-auto overscroll-y-contain`
      )}
      data-slot="carousel-content"
    >
      <div
        className={cn(
          `flex`,
          orientation === `horizontal` ? `-ml-4` : `-mt-4 flex-col`,
          className
        )}
        {...props}
      />
    </div>
  )
}

function CarouselItem({ className, ...props }: React.ComponentProps<`div`>) {
  const { orientation } = useCarousel()

  return (
    <div
      role="group"
      aria-roledescription="slide"
      data-slot="carousel-item"
      className={cn(
        `min-w-0 shrink-0 grow-0 basis-full snap-start`,
        orientation === `horizontal` ? `pl-4` : `pt-4`,
        className
      )}
      {...props}
    />
  )
}

function CarouselPrevious({
  className,
  variant = `outline`,
  size = `icon`,
  ...props
}: React.ComponentProps<typeof Button>) {
  const { orientation, scrollPrev, canScrollPrev } = useCarousel()

  return (
    <Button
      data-slot="carousel-previous"
      variant={variant}
      size={size}
      className={cn(
        `absolute size-8 rounded-full`,
        orientation === `horizontal`
          ? `top-1/2 -left-12 -translate-y-1/2`
          : `-top-12 left-1/2 -translate-x-1/2 rotate-90`,
        className
      )}
      disabled={!canScrollPrev}
      onClick={scrollPrev}
      {...props}
    >
      <ArrowLeftIcon />
      <span className="sr-only">Previous slide</span>
    </Button>
  )
}

function CarouselNext({
  className,
  variant = `outline`,
  size = `icon`,
  ...props
}: React.ComponentProps<typeof Button>) {
  const { orientation, scrollNext, canScrollNext } = useCarousel()

  return (
    <Button
      data-slot="carousel-next"
      variant={variant}
      size={size}
      className={cn(
        `absolute size-8 rounded-full`,
        orientation === `horizontal`
          ? `top-1/2 -right-12 -translate-y-1/2`
          : `-bottom-12 left-1/2 -translate-x-1/2 rotate-90`,
        className
      )}
      disabled={!canScrollNext}
      onClick={scrollNext}
      {...props}
    >
      <ArrowRightIcon />
      <span className="sr-only">Next slide</span>
    </Button>
  )
}

export {
  Carousel,
  CarouselContent,
  CarouselItem,
  CarouselPrevious,
  CarouselNext,
  useCarousel,
}
