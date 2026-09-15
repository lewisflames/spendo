import { describe, expect, it } from 'vitest';
import { bookDisplayName, bookFolder } from './book';

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

describe('bookFolder', () => {
  it('returns the parent folder on POSIX and Windows paths', () => {
    expect(bookFolder('/home/me/Finance/personal.spendo')).toBe('/home/me/Finance');
    expect(bookFolder('C:\\Users\\me\\books\\work.spendo')).toBe('C:\\Users\\me\\books');
  });

  it('returns null at the filesystem root', () => {
    expect(bookFolder('/book.spendo')).toBeNull();
    expect(bookFolder('book.spendo')).toBeNull();
  });
});
