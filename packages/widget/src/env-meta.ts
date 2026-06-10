export interface EnvMeta {
  url: string
  viewportWidth: number
  viewportHeight: number
  screenWidth: number
  screenHeight: number
  devicePixelRatio: number
}

// Collected at submit time so it reflects the state the reporter saw. The
// user agent travels via the request header; the server parses it there.
export function collectEnvMeta(): EnvMeta {
  return {
    url: location.href,
    viewportWidth: window.innerWidth,
    viewportHeight: window.innerHeight,
    screenWidth: window.screen.width,
    screenHeight: window.screen.height,
    devicePixelRatio: window.devicePixelRatio || 1,
  }
}
