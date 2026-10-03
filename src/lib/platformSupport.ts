export type PlatformKind = 'windows' | 'macos' | 'linux';

/** Classify a user-agent string. WebView2, WKWebView and WebKitGTK all include the OS. */
export function detectPlatform(userAgent: string): PlatformKind {
  if (/Windows/i.test(userAgent)) return 'windows';
  if (/Mac OS X|Macintosh/i.test(userAgent)) return 'macos';
  return 'linux';
}

export function currentPlatform(): PlatformKind {
  return typeof navigator === 'undefined' ? 'windows' : detectPlatform(navigator.userAgent);
}

/** Troubleshooting steps shown when device detection fails. */
export function getDetectHints(platform: PlatformKind): string[] {
  const cable = 'Try a different USB cable (some cables are charge-only)';
  const diag = 'Open DIAG to see which ports and PM3 client the app can see';
  switch (platform) {
    case 'macos':
      return [
        cable,
        'Check System Information > USB for the Proxmark3',
        'Install the client: brew install rfidresearchgroup/proxmark3/proxmark3',
        diag,
      ];
    case 'linux':
      return [
        cable,
        'Check ls /dev/ttyACM* and that your user is in the dialout group',
        'ModemManager can grab the port: stop it or add a udev rule',
        diag,
      ];
    case 'windows':
      return [
        cable,
        'Check Device Manager for a COM port (Ports section)',
        'PM3 Easy may need CH340 driver — download from wch-ic.com',
        'Antivirus may block proxmark3.exe — add it to exceptions',
        diag,
      ];
  }
}
