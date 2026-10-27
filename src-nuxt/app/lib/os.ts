export type Os = 'macos' | 'windows' | 'linux'

/** Each system's webview names it in its user agent: WebView2, WKWebView and WebKitGTK. */
export function osOf(userAgent: string): Os {
  if (userAgent.includes('Windows')) {
    return 'windows'
  }
  if (userAgent.includes('Macintosh') || userAgent.includes('Mac OS')) {
    return 'macos'
  }
  return 'linux'
}

/** The system the app runs on, for the names, shortcuts and steps the UI shows. Outside a
 * webview (the unit tests) it is macOS, where the texts were first written. */
export const os: Os = typeof navigator === 'undefined' || !navigator.userAgent.includes('AppleWebKit')
  ? 'macos'
  : osOf(navigator.userAgent)

/** Where apps are and what their files end in, for choosing one in the file dialog: bundles on a
 * Mac, programs and Start menu shortcuts on Windows, launcher entries on Linux. */
export const APP_FILES: { folder: string, extensions: string[] } = {
  macos: { folder: '/Applications', extensions: ['app'] },
  windows: { folder: 'C:\\ProgramData\\Microsoft\\Windows\\Start Menu\\Programs', extensions: ['exe', 'lnk'] },
  linux: { folder: '/usr/share/applications', extensions: ['desktop'] },
}[os]

function parts(path: string) {
  return path.split(/[\\/]/).filter(Boolean)
}

/** The last part of a path, without the extension of an app file. */
export function fileTitle(path: string) {
  const last = parts(path).at(-1) ?? ''
  return last.replace(/\.(app|exe|lnk|desktop)$/i, '')
}

/** The folder a path is in. */
export function parentFolder(path: string) {
  const separator = path.includes('\\') && !path.includes('/') ? '\\' : '/'
  const folder = path.slice(0, Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\')))
  return folder || separator
}

export function isAppFile(path: string) {
  return APP_FILES.extensions.some(ext => path.toLowerCase().endsWith(`.${ext}`))
}
