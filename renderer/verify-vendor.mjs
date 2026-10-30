// Maintainer check: shipped assets must match the exact lockfile installation.
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
for (const [installed, shipped] of [
  ['dist/pele.js', 'pele.js'], ['LICENSE', 'LICENSE'],
]) {
  assert.deepEqual(
    readFileSync(new URL(`../node_modules/pele/${installed}`, import.meta.url)),
    readFileSync(new URL(`../vendor/pele/${shipped}`, import.meta.url)),
    `vendored ${shipped} differs from the pinned npm package`,
  );
}
console.log('Pele bundle and license match the pinned package');
