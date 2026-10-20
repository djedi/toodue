import type { CapacitorConfig } from '@capacitor/cli';

// CAP_DEV_HTTP=1 lets a dev build reach a plain-http local backend
// (Android otherwise blocks http requests from the https://localhost origin).
const devHttp = !!process.env.CAP_DEV_HTTP;

const config: CapacitorConfig = {
  appId: 'com.toodue.toodue',
  appName: 'TooDue',
  webDir: 'dist',
  ...(devHttp && {
    server: { cleartext: true },
    android: { allowMixedContent: true }
  })
};

export default config;
