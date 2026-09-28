import { describe, expect, it } from 'vitest';
import {
  attachmentFileName,
  isAllowedAttachment,
  isImageAttachment,
  selectPendingAttachments
} from './attachment-utils';

describe('attachmentFileName', () => {
  it('returns the last segment for unix and windows paths', () => {
    expect(attachmentFileName('/home/me/receipts/lunch.pdf')).toBe('lunch.pdf');
    expect(attachmentFileName('C:\\Users\\me\\Receipts\\lunch.pdf')).toBe('lunch.pdf');
    expect(attachmentFileName('lunch.pdf')).toBe('lunch.pdf');
  });
});

describe('isAllowedAttachment', () => {
  it('accepts the allowlist case-insensitively', () => {
    expect(isAllowedAttachment('/tmp/a.PDF')).toBe(true);
    expect(isAllowedAttachment('/tmp/a.JpG')).toBe(true);
    expect(isAllowedAttachment('/tmp/a.webp')).toBe(true);
  });

  it('rejects other extensions and extensionless files', () => {
    expect(isAllowedAttachment('/tmp/notes.txt')).toBe(false);
    expect(isAllowedAttachment('/tmp/archive.tar.gz')).toBe(false);
    expect(isAllowedAttachment('/tmp/README')).toBe(false);
  });

  it('ignores dots in directory names', () => {
    expect(isAllowedAttachment('/home/a.b/receipt.pdf')).toBe(true);
    expect(isAllowedAttachment('/home/receipts.pdf/notes')).toBe(false);
  });
});

describe('isImageAttachment', () => {
  it('distinguishes images from pdfs', () => {
    expect(isImageAttachment('/tmp/photo.png')).toBe(true);
    expect(isImageAttachment('/tmp/scan.pdf')).toBe(false);
  });
});

describe('selectPendingAttachments', () => {
  it('adds allowed incoming paths in order', () => {
    const result = selectPendingAttachments([], ['/tmp/b.pdf', '/tmp/a.png']);
    expect(result.added).toEqual(['/tmp/b.pdf', '/tmp/a.png']);
    expect(result.unsupported).toEqual([]);
    expect(result.overflow).toBe(0);
  });

  it('skips duplicates against existing and incoming paths', () => {
    const incoming = ['/tmp/a.pdf', '/tmp/b.png', '/tmp/b.png'];
    const result = selectPendingAttachments(['/tmp/a.pdf'], incoming);
    expect(result.added).toEqual(['/tmp/b.png']);
    expect(result.unsupported).toEqual([]);
    expect(result.overflow).toBe(0);
  });

  it('reports unsupported files without blocking allowed ones', () => {
    const result = selectPendingAttachments([], ['/tmp/notes.txt', '/tmp/a.pdf']);
    expect(result.added).toEqual(['/tmp/a.pdf']);
    expect(result.unsupported).toEqual(['/tmp/notes.txt']);
  });

  it('caps the total and counts the overflow', () => {
    const existing = Array.from({ length: 9 }, (_, i) => `/tmp/${i}.pdf`);
    const result = selectPendingAttachments(existing, ['/tmp/one.pdf', '/tmp/two.pdf']);
    expect(result.added).toEqual(['/tmp/one.pdf']);
    expect(result.overflow).toBe(1);
  });

  it('adds nothing when the list is already full', () => {
    const existing = Array.from({ length: 10 }, (_, i) => `/tmp/${i}.pdf`);
    const result = selectPendingAttachments(existing, ['/tmp/one.pdf']);
    expect(result.added).toEqual([]);
    expect(result.overflow).toBe(1);
  });
});
