// Generates the iOS app icon and launch splash from the shared SVG logo.
import sharp from 'sharp';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const sourceLogo = join(root, 'assets', 'toodue-logo.svg');
const xcassets = join(root, 'ios', 'App', 'App', 'Assets.xcassets');

// Logo centered on an opaque white square (App Store icons can't be transparent).
async function padded(file, size, logoSize) {
  const logo = await sharp(sourceLogo, { density: 1024 }).resize(logoSize, logoSize).png().toBuffer();
  await sharp({ create: { width: size, height: size, channels: 3, background: '#ffffff' } })
    .composite([{ input: logo, gravity: 'center' }])
    .removeAlpha()
    .png()
    .toFile(file);
}

await padded(join(xcassets, 'AppIcon.appiconset', 'AppIcon-512@2x.png'), 1024, 680);
for (const name of ['splash-2732x2732.png', 'splash-2732x2732-1.png', 'splash-2732x2732-2.png']) {
  await padded(join(xcassets, 'Splash.imageset', name), 2732, 400);
}

console.log('native icon and splash written');
