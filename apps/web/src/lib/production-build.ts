// REV-5: security posture must derive from the BUILD, never from
// process.env.NODE_ENV. Vite keeps `process.env` verbatim in the server
// bundle (keepProcessEnv for the `server` consumer) and nothing in the
// shipped image ever set NODE_ENV, so runtime NODE_ENV checks silently
// resolved to their DEV branch in production — Better Auth rate limiting
// off, 1-char passwords accepted, public sign-up open. `import.meta.env.PROD`
// is inlined at build time instead: true in any built output regardless of
// runtime env, false under the dev server and vitest. Keep this module
// dependency-free — small-import-surface route modules (api/contact.ts)
// import it directly.
export const isProductionBuild = import.meta.env.PROD
