import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { readLastAccounts, readRecipes, writeLastAccounts, writeRecipes } from './entry-memory';

function memoryStorage(): Storage {
  const data = new Map<string, string>();
  return {
    get length() {
      return data.size;
    },
    clear: () => data.clear(),
    getItem: (k) => data.get(k) ?? null,
    key: (i) => [...data.keys()][i] ?? null,
    removeItem: (k) => void data.delete(k),
    setItem: (k, v) => void data.set(k, v)
  };
}

describe('entry-memory', () => {
  beforeEach(() => vi.stubGlobal('localStorage', memoryStorage()));
  afterEach(() => vi.unstubAllGlobals());

  it('round-trips last accounts and keeps only strings', () => {
    writeLastAccounts(['a', 'b']);
    expect(readLastAccounts()).toEqual(['a', 'b']);
    localStorage.setItem('tx-last-accounts', JSON.stringify(['a', 1, null]));
    expect(readLastAccounts()).toEqual(['a']);
  });

  it('caps stored lists at 20 entries', () => {
    writeLastAccounts(Array.from({ length: 25 }, (_, i) => `g${i}`));
    expect(readLastAccounts()).toHaveLength(20);
  });

  it('drops malformed recipes and invalid JSON', () => {
    const recipe = { description: 'Rent', splits: [] };
    localStorage.setItem('tx-recent-recipes', JSON.stringify([recipe, { description: 1 }, 'x']));
    expect(readRecipes()).toEqual([recipe]);
    localStorage.setItem('tx-recent-recipes', '{not json');
    expect(readRecipes()).toEqual([]);
    writeRecipes([recipe]);
    expect(readRecipes()).toEqual([recipe]);
  });

  it('returns empty lists when storage is unavailable', () => {
    vi.stubGlobal('localStorage', undefined);
    expect(readLastAccounts()).toEqual([]);
    expect(() => writeRecipes([])).not.toThrow();
  });
});
