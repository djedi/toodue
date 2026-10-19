import { isNative, sessionToken, setSessionToken, apiUrl } from './native.js';

let onUnauthorized = () => {};

export class OfflineError extends Error {
  constructor(message = 'You are offline') {
    super(message);
    this.name = 'OfflineError';
  }
}

export function isOfflineError(err) {
  return err instanceof OfflineError || err?.name === 'OfflineError';
}

export function setUnauthorizedHandler(fn) {
  onUnauthorized = fn;
}

export async function request(method, path, body) {
  const opts = { method, headers: {} };
  const token = sessionToken();
  if (isNative) {
    opts.headers['X-TooDue-Client'] = 'native';
    if (token) opts.headers['Authorization'] = 'Bearer ' + token;
  }
  if (body !== undefined) {
    if (body instanceof FormData) {
      opts.body = body;
    } else {
      opts.headers['Content-Type'] = 'application/json';
      opts.body = JSON.stringify(body);
    }
  }
  let res;
  try {
    res = await fetch(apiUrl(path), opts);
  } catch (err) {
    if (typeof navigator === 'undefined' || navigator.onLine === false || err instanceof TypeError) {
      throw new OfflineError();
    }
    throw err;
  }
  if (res.status === 401) {
    // A stale request finishing after a fresh sign-in mustn't log the user out.
    if (token !== sessionToken()) throw new Error('Not signed in');
    setSessionToken(null);
    onUnauthorized();
    throw new Error('Not signed in');
  }
  if (!res.ok) {
    let msg = 'Something went wrong';
    try {
      msg = (await res.json()).error || msg;
    } catch {}
    throw new Error(msg);
  }
  const ct = res.headers.get('content-type') || '';
  if (!ct.includes('application/json')) return res.text();
  const json = await res.json();
  // Native sign-in responses carry the session token alongside the user.
  if (isNative && json && typeof json === 'object' && typeof json.session_token === 'string') {
    setSessionToken(json.session_token);
    delete json.session_token;
  }
  return json;
}

export const api = {
  get: (p) => request('GET', p),
  post: (p, b) => request('POST', p, b),
  patch: (p, b) => request('PATCH', p, b),
  del: (p) => request('DELETE', p)
};
