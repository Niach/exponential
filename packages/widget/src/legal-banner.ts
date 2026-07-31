// The MIT notices that MUST travel with the emitted bundles (EXP-377).
//
// Build-time only — imported by vite.config.ts, never by loader.ts/main.ts, so
// this module itself never reaches a bundle.
//
// Both artifacts are served cross-origin to CUSTOMER websites, which makes us
// the redistributor: MIT requires the copyright notice to accompany every
// copy. Neither upstream dist carries a `/*!` legal comment of its own
// (checked preact/dist/*.js and @zumer/snapdom/dist/*.mjs — zero legal
// comments), so no bundler `legalComments` setting can preserve what was never
// there. The notice has to be injected, and it goes into BOTH files: loader.js
// is the script the customer embeds, and it is what injects widget.js.
//
// The copyright lines are verbatim from node_modules/preact/LICENSE and
// node_modules/@zumer/snapdom/LICENSE. The full licence texts live in the repo
// NOTICE — a repo URL, deliberately not an instance one, because self-hosters
// serve this widget from their own domain.
//
// `/*!` (not `/*`) is what marks it a legal comment for esbuild; vite.config
// pairs this with `esbuild.legalComments: "inline"` so minification keeps it.
// Gated by legal-banner.test.ts.
export const legalBanner = `/*! Exponential feedback widget · Apache-2.0 · https://github.com/Niach/exponential
 * This widget bundles the following software, both under the MIT License:
 *   preact — Copyright (c) 2015-present Jason Miller
 *   @zumer/snapdom — Copyright (c) 2025 ZumerLab
 * Full licence texts: https://github.com/Niach/exponential/blob/master/NOTICE
 */`
