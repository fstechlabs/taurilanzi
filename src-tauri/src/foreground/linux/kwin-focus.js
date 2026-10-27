// Loaded into KWin while the app runs: reports every window that takes the focus to the app's
// D-Bus object. Plasma 6 names windows `window`, Plasma 5 `client`.
function report(window) {
  if (!window) {
    return
  }
  callDBus('@NAME@', '/com/taurilanzi/Focus', 'com.taurilanzi.Focus', 'Activated',
    String(window.desktopFileName || ''), String(window.resourceClass || ''), String(window.pid || 0))
}

if (typeof workspace.windowActivated !== 'undefined') {
  workspace.windowActivated.connect(report)
  report(workspace.activeWindow)
}
else {
  workspace.clientActivated.connect(report)
  report(workspace.activeClient)
}
