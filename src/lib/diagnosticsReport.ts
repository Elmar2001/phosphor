import type { Pm3Diagnostics } from './api';

const SOURCE_LABEL: Record<string, string> = {
  custom: 'custom path',
  bundled: 'bundled',
  path: 'PATH',
  knownLocation: 'known location',
};

function hex(n: number | null): string {
  return n === null ? '----' : n.toString(16).toUpperCase().padStart(4, '0');
}

/**
 * Plain-text diagnostics for bug reports. Paths are included on purpose:
 * they're the point of the report, and the user sees it before sharing.
 */
export function formatDiagnosticsReport(d: Pm3Diagnostics): string {
  const lines = [
    'Phosphor diagnostics',
    `App: ${d.appVersion}  Platform: ${d.platform}  Overall: ${d.overall}`,
    '',
    'Checks:',
    ...d.checks.map((c) => `  [${c.status}] ${c.label}: ${c.detail}${c.hint ? ` (${c.hint})` : ''}`),
    '',
    'PM3 client:',
    d.client
      ? `  using ${d.client.path} (${SOURCE_LABEL[d.client.source] ?? d.client.source}), version ${d.client.version ?? 'unknown'}`
      : '  none usable',
    ...d.clientCandidates.map(
      (c) => `  ${c.exists ? '[found]  ' : '[missing]'} ${SOURCE_LABEL[c.source] ?? c.source}: ${c.path}`,
    ),
    '',
    'Serial ports:',
    ...(d.ports.length === 0
      ? ['  none']
      : d.ports.map(
          (p) =>
            `  ${p.name} ${p.kind} ${hex(p.vid)}:${hex(p.pid)}${p.product ? ` ${p.product}` : ''}${p.likelyPm3 ? ' <- Proxmark3' : ''}`,
        )),
    '',
    `Firmware: ${d.firmware.map((f) => `${f.variant}=${f.available ? 'yes' : 'no'}`).join(' ')}`,
    `Overrides: port=${d.settings.preferredPort ?? 'auto'} client=${d.settings.clientPath ?? 'auto'}`,
  ];
  return lines.join('\n');
}
