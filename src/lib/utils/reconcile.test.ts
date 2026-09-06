import { describe, expect, it } from 'vitest';
import { statusFor } from './reconcile';

describe('statusFor', () => {
  it('maps GnuCash reconcile states to pills', () => {
    expect(statusFor('y')).toEqual({ label: 'Reconciled', variant: 'success' });
    expect(statusFor('c')).toEqual({ label: 'Cleared', variant: 'warning' });
  });

  it('treats everything else as new', () => {
    expect(statusFor('n')).toEqual({ label: 'New', variant: 'secondary' });
    expect(statusFor(undefined)).toEqual({ label: 'New', variant: 'secondary' });
    expect(statusFor(null)).toEqual({ label: 'New', variant: 'secondary' });
    expect(statusFor('bogus')).toEqual({ label: 'New', variant: 'secondary' });
  });
});
