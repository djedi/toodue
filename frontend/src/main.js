import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { isNative } from './lib/native.js';

import { applyDisplaySize, readDisplaySize } from './lib/displaySize.js';

// Apply before mounting, including auth screens and native tablet layouts.
document.documentElement.dataset.native = String(isNative);
applyDisplaySize(readDisplaySize());

// Native shells ship the assets in the app bundle; no service worker needed.
if (import.meta.env.PROD && !isNative) {
  import('virtual:pwa-register').then(({ registerSW }) => registerSW({ immediate: true }));
}

export default mount(App, { target: document.getElementById('app') });
