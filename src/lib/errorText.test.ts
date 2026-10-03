import { describe, expect, it } from 'vitest';
import { errorText } from './errorText';

describe('errorText', () => {
  it('unwraps externally tagged AppError payloads', () => {
    expect(errorText({ CommandFailed: 'Invalid port: COM0' })).toBe('Invalid port: COM0');
  });

  it('passes through unit variants, strings and Errors', () => {
    expect(errorText('Cancelled')).toBe('Cancelled');
    expect(errorText(new Error('boom'))).toBe('boom');
  });

  it('falls back to JSON for unexpected shapes', () => {
    expect(errorText({ a: 1, b: 2 })).toBe('{"a":1,"b":2}');
  });
});
