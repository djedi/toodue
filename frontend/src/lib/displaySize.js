export const DISPLAY_SIZE_KEY = 'toodue-display-size';
export const DISPLAY_SIZES = [
  { value: 'standard', label: 'Standard', scale: 1 },
  { value: 'large', label: 'Large', scale: 1.125 },
  { value: 'extra-large', label: 'Extra large', scale: 1.25 }
];

export function normalizeDisplaySize(value) {
  return DISPLAY_SIZES.some((size) => size.value === value) ? value : 'standard';
}

export function readDisplaySize(storage) {
  try {
    return normalizeDisplaySize((storage ?? globalThis.localStorage)?.getItem(DISPLAY_SIZE_KEY));
  } catch {
    return 'standard';
  }
}

// Attribute-based CSS keeps desktop unchanged and responds to viewport changes.
export function applyDisplaySize(value, root = globalThis.document?.documentElement) {
  const size = normalizeDisplaySize(value);
  if (root) root.dataset.displaySize = size;
  return size;
}

export function saveDisplaySize(value, storage, root) {
  const size = applyDisplaySize(value, root);
  try {
    (storage ?? globalThis.localStorage)?.setItem(DISPLAY_SIZE_KEY, size);
  } catch {
    // Restricted storage must not prevent the immediate in-session preview.
  }
  return size;
}
