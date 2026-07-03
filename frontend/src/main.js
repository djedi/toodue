import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';

if (import.meta.env.PROD) {
  import('virtual:pwa-register').then(({ registerSW }) => registerSW({ immediate: true }));
}

export default mount(App, { target: document.getElementById('app') });
