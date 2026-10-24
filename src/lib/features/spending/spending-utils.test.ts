import { describe, expect, it } from 'vitest';
import type { SpendingCategory } from '$lib/tauri';
import {
  buildSpendingRows,
  buildSpendingSeries,
  categoryLabel,
  OTHER_COLOR,
  OTHER_KEY,
  seriesColorByGuid,
  sharePct
} from './spending-utils';

const cat = (guid: string, monthly: number[], extra: Partial<SpendingCategory> = {}) => ({
  accountGuid: guid,
  accountName: guid.toUpperCase(),
  hasChildren: false,
  ownSplits: false,
  monthly,
  total: monthly.reduce((a, b) => a + b, 0),
  previousTotal: 0,
  changePct: null,
  ...extra
});

describe('buildSpendingSeries', () => {
  it('gives each category its own slot while they fit', () => {
    const series = buildSpendingSeries([cat('a', [3]), cat('b', [2])]);
    expect(series.map((s) => [s.key, s.color])).toEqual([
      ['c0', 'var(--series-1)'],
      ['c1', 'var(--series-2)']
    ]);
  });

  it('folds everything past seven into Other', () => {
    const cats = Array.from({ length: 10 }, (_, i) => cat(`k${i}`, [10 - i]));
    const series = buildSpendingSeries(cats);
    expect(series).toHaveLength(8);
    expect(series[6].color).toBe('var(--series-7)');
    expect(series[7]).toMatchObject({ key: OTHER_KEY, color: OTHER_COLOR });
    expect(series[7].guids).toEqual(['k7', 'k8', 'k9']);
  });

  it('keeps exactly eight categories without an Other bucket', () => {
    const cats = Array.from({ length: 8 }, (_, i) => cat(`k${i}`, [1]));
    expect(buildSpendingSeries(cats).map((s) => s.key)).not.toContain(OTHER_KEY);
  });

  it('leaves out categories with no activity in the window', () => {
    const series = buildSpendingSeries([cat('a', [5]), cat('prev', [0], { previousTotal: 9 })]);
    expect(series.flatMap((s) => s.guids)).toEqual(['a']);
  });
});

describe('buildSpendingRows', () => {
  it('sums folded categories per month', () => {
    const cats = Array.from({ length: 9 }, (_, i) => cat(`k${i}`, [9 - i, 1]));
    const series = buildSpendingSeries(cats);
    const rows = buildSpendingRows({ months: ['2024-01', '2024-02'], categories: cats }, series);
    expect(rows[0]).toMatchObject({ month: '2024-01', c0: 9, [OTHER_KEY]: 2 + 1 });
    expect(rows[1][OTHER_KEY]).toBe(2);
  });
});

describe('helpers', () => {
  it('maps folded categories to the Other color', () => {
    const cats = Array.from({ length: 9 }, (_, i) => cat(`k${i}`, [9 - i]));
    const colors = seriesColorByGuid(buildSpendingSeries(cats));
    expect(colors.get('k0')).toBe('var(--series-1)');
    expect(colors.get('k8')).toBe(OTHER_COLOR);
  });

  it('labels a parent’s own splits', () => {
    expect(categoryLabel(cat('food', [1], { ownSplits: true }))).toBe('FOOD (direct)');
  });

  it('computes shares only against a positive total', () => {
    expect(sharePct(25, 100)).toBe(25);
    expect(sharePct(5, 0)).toBeNull();
  });
});
