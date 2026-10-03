import { useCallback, useEffect, useState, type CSSProperties } from 'react';
import { getPm3Settings, listSerialPorts, setPm3Settings, type SerialPortEntry } from '../../lib/api';
import { errorText } from '../../lib/errorText';
import { useSfx } from '../../hooks/useSfx';

const AUTO = '';

const fieldStyle: CSSProperties = {
  marginTop: '4px',
  width: '100%',
  background: 'var(--bg-void)',
  border: '1px solid var(--green-dim)',
  color: 'var(--green-bright)',
  fontFamily: 'var(--font-mono)',
  fontSize: '12px',
  padding: '6px 8px',
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

function portLabel(port: SerialPortEntry): string {
  const what = port.likelyPm3 ? 'Proxmark3' : port.product ?? port.manufacturer ?? port.kind;
  return `${port.name} — ${what}`;
}

/**
 * Port and client-path overrides. Saved to the backend (SQLite) so detection
 * uses them from startup; the backend validates both and the error is shown.
 */
export function Pm3RuntimeSettings() {
  const sfx = useSfx();
  const [ports, setPorts] = useState<SerialPortEntry[]>([]);
  const [portsError, setPortsError] = useState<string | null>(null);
  const [port, setPort] = useState(AUTO);
  const [clientPath, setClientPath] = useState('');
  const [saved, setSaved] = useState({ port: AUTO, clientPath: '' });
  const [status, setStatus] = useState<{ text: string; error: boolean } | null>(null);
  const [busy, setBusy] = useState(false);

  const refreshPorts = useCallback(async () => {
    try {
      setPorts(await listSerialPorts());
      setPortsError(null);
    } catch (err) {
      setPortsError(errorText(err));
    }
  }, []);

  useEffect(() => {
    getPm3Settings()
      .then((s) => {
        const loaded = { port: s.preferredPort ?? AUTO, clientPath: s.clientPath ?? '' };
        setPort(loaded.port);
        setClientPath(loaded.clientPath);
        setSaved(loaded);
      })
      .catch((err) => setStatus({ text: errorText(err), error: true }));
    refreshPorts();
  }, [refreshPorts]);

  const dirty = port !== saved.port || clientPath.trim() !== saved.clientPath;

  const save = async () => {
    sfx.click();
    setBusy(true);
    try {
      const result = await setPm3Settings({
        preferredPort: port || null,
        clientPath: clientPath.trim() || null,
      });
      const next = { port: result.preferredPort ?? AUTO, clientPath: result.clientPath ?? '' };
      setPort(next.port);
      setClientPath(next.clientPath);
      setSaved(next);
      setStatus({ text: 'Saved. Applies to the next detection.', error: false });
    } catch (err) {
      setStatus({ text: errorText(err), error: true });
    } finally {
      setBusy(false);
    }
  };

  // Keep a saved port selectable even while it's unplugged.
  const savedPortMissing = saved.port !== AUTO && !ports.some((p) => p.name === saved.port);

  return (
    <div>
      <div style={{ color: 'var(--green-mid)', fontSize: '13px', fontWeight: 600 }}>
        PM3 RUNTIME
      </div>
      <div style={{ color: 'var(--green-dim)', fontSize: '12px', marginTop: '4px' }}>
        Optional overrides. Leave on auto unless detection picks the wrong port or can't find the client.
      </div>

      <label style={{ display: 'block', marginTop: '10px', fontSize: '12px', color: 'var(--green-dim)' }}>
        PREFERRED PORT
      </label>
      <div style={{ display: 'flex', gap: '8px', alignItems: 'flex-end' }}>
        <select value={port} onChange={(e) => setPort(e.target.value)} style={fieldStyle}>
          <option value={AUTO}>auto (Proxmark3 USB devices first)</option>
          {savedPortMissing && <option value={saved.port}>{saved.port} — not connected</option>}
          {ports.map((p) => (
            <option key={p.name} value={p.name}>
              {portLabel(p)}
            </option>
          ))}
        </select>
        <button onClick={() => { sfx.click(); refreshPorts(); }} style={buttonStyle(true)}>
          [RESCAN]
        </button>
      </div>
      {portsError && (
        <div style={{ color: 'var(--amber)', fontSize: '12px', marginTop: '4px' }}>
          Can't list ports: {portsError}
        </div>
      )}

      <label style={{ display: 'block', marginTop: '10px', fontSize: '12px', color: 'var(--green-dim)' }}>
        PM3 CLIENT PATH
      </label>
      <input
        value={clientPath}
        onChange={(e) => setClientPath(e.target.value)}
        placeholder="auto (bundled, PATH, Homebrew, ...)"
        spellCheck={false}
        style={fieldStyle}
      />

      <div style={{ display: 'flex', gap: '12px', alignItems: 'center', marginTop: '10px' }}>
        <button onClick={save} disabled={!dirty || busy} style={buttonStyle(dirty && !busy)}>
          {busy ? '[SAVING]' : '[SAVE]'}
        </button>
        {status && (
          <span style={{ fontSize: '12px', color: status.error ? 'var(--red-bright)' : 'var(--green-mid)' }}>
            {status.error ? '[!!] ' : ''}{status.text}
          </span>
        )}
      </div>
    </div>
  );
}
