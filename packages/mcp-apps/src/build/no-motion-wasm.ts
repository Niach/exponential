// #NEW-140: the views without genUI surfaces never draw a Motion or a Canvas;
// their documents carry no engine (vite.config.ts `leanMotionWasm`).
export function loadMotionWasm(): Promise<never> {
  return Promise.reject(new Error(`Motion: not in this view`))
}
