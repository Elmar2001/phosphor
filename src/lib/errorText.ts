/**
 * Readable text for a rejected invoke(). Rust's AppError serializes
 * externally tagged: {"CommandFailed": "msg"}, or a bare "Variant" string
 * for unit variants.
 */
export function errorText(err: unknown): string {
  if (typeof err === 'string') return err;
  if (err instanceof Error) return err.message;
  if (err && typeof err === 'object') {
    const values = Object.values(err as Record<string, unknown>);
    if (values.length === 1 && typeof values[0] === 'string') return values[0];
    return JSON.stringify(err);
  }
  return String(err);
}
