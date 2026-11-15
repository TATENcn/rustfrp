/**
 * Desktop-shell detection.
 *
 * The Tauri shell injects `window.__RUSTFRP_DESKTOP__` and a bearer token before
 * the bundle runs. There is no separate desktop build: the same assets serve the
 * browser, the container, and the desktop window, so features that only make
 * sense inside the shell (no logout, no token prompt) are gated at runtime.
 */
declare global {
  interface Window {
    __RUSTFRP_DESKTOP__?: boolean;
  }
}

export function isDesktop(): boolean {
  return window.__RUSTFRP_DESKTOP__ === true;
}
