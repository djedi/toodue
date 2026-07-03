export function maskApiKey(key) {
  if (!key) return '';
  if (key.length <= 12) return key;
  return `${key.slice(0, 7)}…${key.slice(-4)}`;
}

export function apiKeyCreatedMessage(name) {
  return `API key “${name}” created. Copy it now — it will only be shown once.`;
}
