import { Capacitor } from '@capacitor/core';

// Native (Capacitor) builds bundle the SPA locally, so the API lives on another
// origin. Set VITE_API_BASE (e.g. https://toodue.example.com) when building them.
// Cookies don't cross origins, so native builds authenticate with a session
// token sent as a bearer header instead.
export const isNative = Capacitor.isNativePlatform();
export const API_BASE = (import.meta.env.VITE_API_BASE || '').replace(/\/+$/, '');

const TOKEN_KEY = 'toodue-session-token';

export function sessionToken() {
  if (!isNative) return null;
  try {
    return localStorage.getItem(TOKEN_KEY);
  } catch {
    return null;
  }
}

export function setSessionToken(token) {
  if (!isNative) return;
  try {
    if (token) localStorage.setItem(TOKEN_KEY, token);
    else localStorage.removeItem(TOKEN_KEY);
  } catch {}
}

// Absolute URL for an API path, for places that can't go through request().
// withToken appends the session token; only for in-app use (the SSE stream).
export function apiUrl(path, { withToken = false } = {}) {
  const url = API_BASE + '/api' + path;
  const token = withToken && sessionToken();
  if (!token) return url;
  return url + (url.includes('?') ? '&' : '?') + 'access_token=' + encodeURIComponent(token);
}

// Click handler for links to authenticated API URLs (downloads, OAuth redirects).
// On the web the cookie covers them. Native opens them in the system browser,
// which has no session, so it trades for a short-lived single-use link token.
export async function openApiLink(event, path, post) {
  if (!isNative) return;
  event.preventDefault();
  const { token } = await post('/auth/link-token', { path });
  window.location.href = apiUrl(path) + '?link_token=' + encodeURIComponent(token);
}
