import { describe, expect, it } from 'vitest';
import {
  addTagName,
  MAX_TAG_LEN,
  MAX_TAGS_PER_TX,
  normalizeTagName,
  sameTags,
  tagNameError,
  tagSuggestions
} from './tag-utils';

describe('normalizeTagName', () => {
  it('trims and collapses whitespace', () => {
    expect(normalizeTagName('  Summer   trip ')).toBe('Summer trip');
    expect(normalizeTagName('   ')).toBe('');
  });
});

describe('tagNameError', () => {
  it('rejects empty, long and comma names', () => {
    expect(tagNameError('')).not.toBeNull();
    expect(tagNameError('x'.repeat(MAX_TAG_LEN + 1))).not.toBeNull();
    expect(tagNameError('a,b')).not.toBeNull();
    expect(tagNameError('x'.repeat(MAX_TAG_LEN))).toBeNull();
  });
});

describe('addTagName', () => {
  it('reuses a known spelling and ignores duplicates', () => {
    expect(addTagName([], ' deductible ', ['Deductible']).tags).toEqual(['Deductible']);
    expect(addTagName(['Trip'], 'trip')).toEqual({ tags: ['Trip'], error: null });
  });

  it('reports invalid names and the cap without changing the list', () => {
    expect(addTagName(['a'], 'b,c')).toEqual({ tags: ['a'], error: expect.any(String) });
    const full = Array.from({ length: MAX_TAGS_PER_TX }, (_, i) => `t${i}`);
    expect(addTagName(full, 'more').error).not.toBeNull();
    expect(addTagName(full, 't0').error).toBeNull();
  });
});

describe('tagSuggestions', () => {
  const known = ['work', 'Summer trip', 'trip-2024', 'Deductible'];

  it('drops picked tags and puts prefix matches first', () => {
    expect(tagSuggestions(known, ['WORK'], '')).toEqual(['Summer trip', 'trip-2024', 'Deductible']);
    expect(tagSuggestions(known, [], 'trip')).toEqual(['trip-2024', 'Summer trip']);
    expect(tagSuggestions(known, [], 'nope')).toEqual([]);
  });

  it('honors the limit', () => {
    expect(tagSuggestions(known, [], '', 2)).toHaveLength(2);
  });
});

describe('sameTags', () => {
  it('ignores order', () => {
    expect(sameTags(['a', 'b'], ['b', 'a'])).toBe(true);
    expect(sameTags(['a'], ['a', 'b'])).toBe(false);
  });
});
