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
    res = await fetch('/api' + path, opts);
  } catch (err) {
    if (typeof navigator === 'undefined' || navigator.onLine === false || err instanceof TypeError) {
      throw new OfflineError();
    }
    throw err;
  }
  if (res.status === 401) {
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
  return ct.includes('application/json') ? res.json() : res.text();
}

export const api = {
  get: (p) => request('GET', p),
  post: (p, b) => request('POST', p, b),
  patch: (p, b) => request('PATCH', p, b),
  del: (p) => request('DELETE', p)
};
