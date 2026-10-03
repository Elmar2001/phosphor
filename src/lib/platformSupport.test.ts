import { describe, expect, it } from 'vitest';
import { detectPlatform, getDetectHints } from './platformSupport';

describe('detectPlatform', () => {
  it('recognizes the webview user agents', () => {
    expect(detectPlatform('Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Edg/120.0')).toBe('windows');
    expect(detectPlatform('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15')).toBe('macos');
    expect(detectPlatform('Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15')).toBe('linux');
  });
});

describe('getDetectHints', () => {
  it('keeps the Windows driver and antivirus advice', () => {
    const hints = getDetectHints('windows');
    expect(hints.some((h) => h.includes('Device Manager'))).toBe(true);
    expect(hints.some((h) => h.includes('Antivirus'))).toBe(true);
  });

  it('gives macOS and Linux their own steps', () => {
    expect(getDetectHints('macos').some((h) => h.includes('brew install'))).toBe(true);
    expect(getDetectHints('linux').some((h) => h.includes('dialout'))).toBe(true);
    expect(getDetectHints('linux').some((h) => h.includes('Device Manager'))).toBe(false);
  });
});
