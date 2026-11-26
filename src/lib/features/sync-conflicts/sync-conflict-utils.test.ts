import { describe, expect, it } from 'vitest';
import type { SyncConflictCopy } from '$lib/tauri';
import {
  MAX_DISMISSED,
  conflictFieldLabel,
  parseDismissed,
  visibleConflicts,
  withDismissed
} from './sync-conflict-utils';

const copy = (path: string): SyncConflictCopy => ({
  path,
  fileName: path.split('/').pop() ?? path,
  savedAt: '2026-11-25T14:30:22',
  device: 'ABCDEF7'
});

describe('parseDismissed', () => {
  it('keeps only non-empty strings', () => {
    expect(parseDismissed(['/a', 3, '', null, '/b'])).toEqual(['/a', '/b']);
  });

  it('drops anything that is not a list', () => {
    expect(parseDismissed({ path: '/a' })).toEqual([]);
    expect(parseDismissed('/a')).toEqual([]);
  });

  it('caps the list at the newest entries', () => {
    const many = Array.from({ length: MAX_DISMISSED + 5 }, (_, i) => `/c${i}`);
    const parsed = parseDismissed(many);
    expect(parsed).toHaveLength(MAX_DISMISSED);
    expect(parsed.at(-1)).toBe(`/c${MAX_DISMISSED + 4}`);
  });
});

describe('withDismissed', () => {
  it('adds a path once, at the end', () => {
    expect(withDismissed(['/a', '/b'], '/a')).toEqual(['/b', '/a']);
  });
});

describe('visibleConflicts', () => {
  it('hides dismissed copies and keeps the order', () => {
    const copies = [copy('/s/b.sync-conflict-2.spendo'), copy('/s/b.sync-conflict-1.spendo')];
    expect(visibleConflicts(copies, ['/s/b.sync-conflict-2.spendo'])).toEqual([copies[1]]);
    expect(visibleConflicts(copies, [])).toEqual(copies);
  });
});

describe('conflictFieldLabel', () => {
  it('names every field in plain words', () => {
    expect(conflictFieldLabel('splits')).toBe('Amounts or accounts');
    expect(conflictFieldLabel('status')).toBe('Reconcile status');
  });
});
