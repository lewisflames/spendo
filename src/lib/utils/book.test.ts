import { describe, expect, it } from 'vitest';
import { bookDisplayName } from './book';

describe('bookDisplayName', () => {
  it('strips directories and the .spendo extension', () => {
    expect(bookDisplayName('/home/me/Finance/personal.spendo')).toBe('personal');
    expect(bookDisplayName('C:\\Users\\me\\books\\work.SPENDO')).toBe('work');
  });

  it('keeps other extensions and extension-less names intact', () => {
    expect(bookDisplayName('/tmp/old.db')).toBe('old.db');
    expect(bookDisplayName('/tmp/untitled')).toBe('untitled');
  });

  it('falls back for a missing path', () => {
    expect(bookDisplayName(null)).toBe('No book open');
  });
});
