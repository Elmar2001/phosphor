import { describe, expect, it } from 'vitest';
import { formatDiagnosticsReport } from './diagnosticsReport';
import type { Pm3Diagnostics } from './api';

const sample: Pm3Diagnostics = {
  appVersion: '1.1.0',
  platform: 'macos aarch64',
  overall: 'warning',
  checks: [
    { id: 'client', label: 'PM3 client', status: 'ok', detail: 'Iceman/master/v4.20728', hint: null },
    {
      id: 'firmware',
      label: 'Bundled firmware',
      status: 'warning',
      detail: 'No firmware images bundled',
      hint: 'Flash manually',
    },
  ],
  client: { path: '/opt/homebrew/bin/proxmark3', source: 'knownLocation', version: 'Iceman/master/v4.20728' },
  clientCandidates: [
    { source: 'path', path: '/usr/local/bin/proxmark3', exists: false },
    { source: 'knownLocation', path: '/opt/homebrew/bin/proxmark3', exists: true },
  ],
  ports: [
    {
      name: '/dev/tty.usbmodemiceman1',
      kind: 'usb',
      vid: 0x9ac4,
      pid: 0x4b8f,
      manufacturer: 'proxmark.org',
      product: 'proxmark3',
      likelyPm3: true,
    },
  ],
  firmware: [
    { variant: 'rdv4', available: false },
    { variant: 'generic', available: false },
  ],
  settings: { preferredPort: null, clientPath: null },
};

describe('formatDiagnosticsReport', () => {
  const report = formatDiagnosticsReport(sample);

  it('summarizes app, platform and overall status', () => {
    expect(report).toContain('App: 1.1.0  Platform: macos aarch64  Overall: warning');
  });

  it('lists checks with hints', () => {
    expect(report).toContain('[warning] Bundled firmware: No firmware images bundled (Flash manually)');
    expect(report).toContain('[ok] PM3 client: Iceman/master/v4.20728');
  });

  it('shows the client in use and every lookup candidate', () => {
    expect(report).toContain('using /opt/homebrew/bin/proxmark3 (known location), version Iceman/master/v4.20728');
    expect(report).toContain('[missing] PATH: /usr/local/bin/proxmark3');
  });

  it('formats USB IDs and flags the Proxmark3 port', () => {
    expect(report).toContain('/dev/tty.usbmodemiceman1 usb 9AC4:4B8F proxmark3 <- Proxmark3');
  });

  it('reports firmware and overrides', () => {
    expect(report).toContain('Firmware: rdv4=no generic=no');
    expect(report).toContain('Overrides: port=auto client=auto');
  });

  it('handles a missing client and no ports', () => {
    const empty = formatDiagnosticsReport({ ...sample, client: null, ports: [] });
    expect(empty).toContain('  none usable');
    expect(empty).toMatch(/Serial ports:\n {2}none/);
  });
});
