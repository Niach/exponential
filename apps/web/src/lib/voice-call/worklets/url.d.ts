// vite/client types `?url` and `?no-inline`, not the two together.
declare module "*?url&no-inline" {
  const src: string
  export default src
}
