import { describe, expect, it } from 'vitest';
import { errKind, errMsg, isBackendError } from './error';

describe('backend errors', () => {
  const notFound = { kind: 'notFound', message: 'Holding not found: c-1' };

  it('recognizes serialized AppError payloads only', () => {
    expect(isBackendError(notFound)).toBe(true);
    expect(isBackendError({ kind: 'teapot', message: 'x' })).toBe(false);
    expect(isBackendError({ kind: 'db' })).toBe(false);
    expect(isBackendError('Holding not found')).toBe(false);
    expect(isBackendError(null)).toBe(false);
  });

  it('reads the message from backend errors, Errors and strings', () => {
    expect(errMsg(notFound, 'fallback')).toBe('Holding not found: c-1');
    expect(errMsg(new Error('boom'), 'fallback')).toBe('boom');
    expect(errMsg('plain', 'fallback')).toBe('plain');
    expect(errMsg(42, 'fallback')).toBe('fallback');
  });

  it('exposes the kind only for backend errors', () => {
    expect(errKind(notFound)).toBe('notFound');
    expect(errKind(new Error('Holding not found'))).toBeNull();
  });
});
