// Embedded by Rust. JSON stdin keeps the vendored bundle out of argv and disk.
import { readFileSync } from 'node:fs';
try {
  if (Number(process.versions.node.split('.')[0]) < 20) {
    throw new Error('Node >=20 is required');
  }
  const { bundle, source, prefix = 'atlas' } = JSON.parse(readFileSync(0, 'utf8'));
  if (typeof bundle !== 'string' || typeof source !== 'string') {
    throw new Error('expected bundle and source strings');
  }
  const url = `data:text/javascript;base64,${Buffer.from(bundle).toString('base64')}`;
  const { render } = await import(url);
  const { svg } = render(source, {
    idPrefix: prefix, autoDirection: false, responsive: false,
    links: false, images: false,
  });
  process.stdout.write(svg);
} catch (error) {
  console.error(`atlas Pele: ${error.message}`);
  process.exitCode = 1;
}
