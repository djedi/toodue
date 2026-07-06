// Generates the TooDue PWA icon set from the shared SVG logo.
import sharp from 'sharp';
import { mkdirSync, copyFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const outDir = join(scriptDir, '..', 'public', 'icons');
const sourceLogo = join(scriptDir, '..', 'assets', 'toodue-logo.svg');
mkdirSync(outDir, { recursive: true });

copyFileSync(sourceLogo, join(outDir, 'toodue-logo.svg'));
copyFileSync(sourceLogo, join(outDir, 'favicon.svg'));

async function png(name, size) {
  await sharp(sourceLogo, { density: 1024 })
    .resize(size, size, { fit: 'contain' })
    .png()
    .toFile(join(outDir, name));
}

await png('pwa-192.png', 192);
await png('pwa-512.png', 512);
await png('maskable-512.png', 512);
await png('apple-touch-icon.png', 180);

console.log(`icons written to ${outDir}`);
