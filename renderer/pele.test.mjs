import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { readFileSync } from 'node:fs';
const bundle = readFileSync(new URL('../vendor/pele/pele.js', import.meta.url), 'utf8');
const request = source => JSON.stringify({ bundle, source });
const adapter = fileURLToPath(new URL('./pele.mjs', import.meta.url));
test('local adapter renders the labelled edge population without DOM', () => {
  const result = spawnSync(process.execPath, [adapter], {
    input: request('flowchart LR\n subgraph g["src/backend/"]\n a["emit"]\n b["Executable"]\n end\n a -->|produces x1| b\n'), encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /<svg/);
  assert.match(result.stdout, /emit/);
  assert.match(result.stdout, /Executable/);
  assert.match(result.stdout, /produces x1/);
  assert.doesNotMatch(result.stdout, /<script/);
});
test('malformed Mermaid produces a named error and no partial SVG', () => {
  const result = spawnSync(process.execPath, [adapter], {input: request('not a diagram'), encoding: 'utf8'});
  assert.equal(result.status, 1);
  assert.equal(result.stdout, '');
  assert.match(result.stderr, /atlas Pele:/);
});
