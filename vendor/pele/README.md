# Pele runtime

`pele.js` is the unmodified standalone ESM bundle from `pele@1.8.0`,
`dist/pele.js`. SHA-256: `30d94f823025c873abb2ec3676e34124e543583731e228ca957f51ddda415a05`.
The upstream MIT license and ISC icon notice are preserved in `LICENSE`.

The bundle has no imports or external runtime dependencies. Atlas embeds it and
its small bridge in the Rust binary. Only Node >=20 is required for SVG/HTML;
ASCII and source queries require no JavaScript runtime.

To deliberately update: change the exact Pele version with npm, run `npm ci`,
copy `node_modules/pele/dist/pele.js` and `node_modules/pele/LICENSE` here,
update this checksum, then run `npm test` and `npm run verify-vendor` and the Rust
checks. Commit the package lock, bundle, license and provenance together.

`npm ci` is a maintainer verification step, never a user runtime setup step.
