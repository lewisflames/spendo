import { describe, expect, it } from 'vitest';
import { errMsg } from './error';

describe('errMsg', () => {
  it('unwraps Error instances', () => {
    expect(errMsg(new Error('boom'), 'fallback')).toBe('boom');
  });

  it('passes strings through', () => {
    expect(errMsg('plain failure', 'fallback')).toBe('plain failure');
  });

  it('falls back for anything else', () => {
    expect(errMsg(null, 'fallback')).toBe('fallback');
    expect(errMsg(undefined, 'fallback')).toBe('fallback');
    expect(errMsg(42, 'fallback')).toBe('fallback');
    expect(errMsg({ message: 'not an Error' }, 'fallback')).toBe('fallback');
  });
});
