import { Capacitor, registerPlugin } from '@capacitor/core';

// iOS draws the bottom tabs natively (Liquid Glass) via the NativeTabs plugin
// in ios/App/App/MainViewController.swift. Everywhere else uses BottomNav's web tabs.
// It's registered at runtime (registerPluginInstance), which isPluginAvailable()
// doesn't see, so BottomNav falls back to the web tabs if a call rejects.
export const nativeTabsEnabled = Capacitor.getPlatform() === 'ios';

export const NativeTabs = nativeTabsEnabled ? registerPlugin('NativeTabs') : null;

// The native bar sits above the web view, so it would cover web dialogs and
// bottom sheets. Report whether any full-screen overlay is open.
export function watchOverlays(onChange) {
  const check = () => onChange(!!document.querySelector('.fixed.inset-0'));
  const observer = new MutationObserver(check);
  observer.observe(document.body, { childList: true, subtree: true });
  check();
  return () => observer.disconnect();
}
