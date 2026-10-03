import { useCallback, useEffect, useState, type CSSProperties, type ReactNode } from 'react';
import { getPm3Diagnostics, type CheckStatus, type DiagnosticCheck, type Pm3Diagnostics } from '../../lib/api';
import { formatDiagnosticsReport } from '../../lib/diagnosticsReport';
import { errorText } from '../../lib/errorText';
import { useSfx } from '../../hooks/useSfx';
import { TerminalPanel } from '../shared/TerminalPanel';

const STATUS_COLOR: Record<CheckStatus, string> = {
  ok: 'var(--green-bright)',
  warning: 'var(--amber)',
  error: 'var(--red-bright)',
};

const buttonStyle = (enabled: boolean): CSSProperties => ({
  background: 'var(--bg-void)',
  border: '1px solid var(--green-dim)',
  color: enabled ? 'var(--green-bright)' : 'var(--green-dim)',
  fontFamily: 'var(--font-mono)',
  fontSize: '12px',
  padding: '4px 10px',
  cursor: enabled ? 'pointer' : 'default',
});

/**
 * Setup checks that don't touch the device: is the PM3 client runnable,
 * which serial ports exist, is firmware bundled, do overrides still apply.
 */
export function DiagnosticsView() {
  const sfx = useSfx();
  const [diagnostics, setDiagnostics] = useState<Pm3Diagnostics | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'failed'>('idle');

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setDiagnostics(await getPm3Diagnostics());
    } catch (err) {
      setError(errorText(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const copyReport = async () => {
    if (!diagnostics) return;
    sfx.click();
    try {
      await navigator.clipboard.writeText(formatDiagnosticsReport(diagnostics));
      setCopyState('copied');
    } catch {
      setCopyState('failed');
    }
    setTimeout(() => setCopyState('idle'), 2000);
  };

  const copyLabel = copyState === 'copied' ? '[COPIED]' : copyState === 'failed' ? '[COPY FAILED]' : '[COPY REPORT]';

  return (
    <TerminalPanel title="DIAGNOSTICS">
      <div style={{ display: 'flex', flexDirection: 'column', gap: '14px', fontSize: '12px', lineHeight: 1.6 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
          {diagnostics && (
            <span style={{ color: STATUS_COLOR[diagnostics.overall], fontWeight: 700 }}>
              STATUS: {diagnostics.overall.toUpperCase()}
            </span>
          )}
          <span style={{ marginLeft: 'auto', display: 'flex', gap: '8px' }}>
            <button onClick={() => { sfx.click(); load(); }} disabled={loading} style={buttonStyle(!loading)}>
              {loading ? '[RUNNING]' : '[REFRESH]'}
            </button>
            <button onClick={copyReport} disabled={!diagnostics} style={buttonStyle(!!diagnostics)}>
              {copyLabel}
            </button>
          </span>
        </div>

        {error && <div style={{ color: 'var(--red-bright)' }}>[!!] {error}</div>}

        {diagnostics && (
          <>
            <Section title="CHECKS">
              {diagnostics.checks.map((check) => (
                <CheckRow key={check.id} check={check} />
              ))}
            </Section>

            <Section title="PM3 CLIENT LOOKUP">
              {diagnostics.clientCandidates.map((c) => {
                const inUse = diagnostics.client?.path === c.path;
                return (
                  <div key={c.path} style={{ color: inUse ? 'var(--green-bright)' : c.exists ? 'var(--green-mid)' : 'var(--green-dim)' }}>
                    {inUse ? '[USING]' : c.exists ? '[FOUND]' : '[ -- ]'} {c.path}
                  </div>
                );
              })}
            </Section>

            <Section title="SERIAL PORTS">
              {diagnostics.ports.length === 0 && <div style={{ color: 'var(--green-dim)' }}>none</div>}
              {diagnostics.ports.map((p) => (
                <div key={p.name} style={{ color: p.likelyPm3 ? 'var(--green-bright)' : 'var(--green-dim)' }}>
                  {p.likelyPm3 ? '[PM3]' : '[   ]'} {p.name} {p.product ?? p.kind}
                </div>
              ))}
            </Section>

            <Section title="SYSTEM">
              <div style={{ color: 'var(--green-dim)' }}>
                Phosphor {diagnostics.appVersion} on {diagnostics.platform}
                {' | '}firmware: {diagnostics.firmware.map((f) => `${f.variant}${f.available ? '' : ' (missing)'}`).join(', ')}
              </div>
            </Section>
          </>
        )}
      </div>
    </TerminalPanel>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div>
      <div style={{ color: 'var(--green-mid)', fontWeight: 700, marginBottom: '4px' }}>{title}</div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: '3px', wordBreak: 'break-all' }}>{children}</div>
    </div>
  );
}

function CheckRow({ check }: { check: DiagnosticCheck }) {
  return (
    <div>
      <span style={{ color: STATUS_COLOR[check.status], fontWeight: 700 }}>[{check.status.toUpperCase()}]</span>{' '}
      <span style={{ color: 'var(--green-bright)' }}>{check.label}</span>
      <span style={{ color: 'var(--green-dim)' }}> - {check.detail}</span>
      {check.hint && <div style={{ color: 'var(--amber)', paddingLeft: '24px' }}>{check.hint}</div>}
    </div>
  );
}
