import { beforeEach, describe, expect, it, vi } from 'vitest';
import { toast } from '$lib/components/ui/sonner';
import { todayStr } from '$lib/utils/date-utils.svelte';
import {
  holdingLabel,
  priceStaleDays,
  dayChangePct,
  range52w,
  toastPriceResult
} from './price-utils';

vi.mock('$lib/components/ui/sonner', () => ({
  toast: { success: vi.fn(), warning: vi.fn(), error: vi.fn() }
}));

const mockedToast = vi.mocked(toast);

beforeEach(() => {
  vi.clearAllMocks();
});

describe('holdingLabel', () => {
  it('prefers the full name, falls back to the mnemonic', () => {
    expect(holdingLabel({ fullName: 'Apple Inc', mnemonic: 'AAPL' })).toBe('Apple Inc');
    expect(holdingLabel({ fullName: null, mnemonic: 'AAPL' })).toBe('AAPL');
  });
});

describe('priceStaleDays', () => {
  it('reports zero for today', () => {
    expect(priceStaleDays(todayStr())).toBe(0);
  });

  it('counts whole calendar days', () => {
    const now = new Date();
    const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 7);
    const m = String(yesterday.getMonth() + 1).padStart(2, '0');
    const d = String(yesterday.getDate()).padStart(2, '0');
    expect(priceStaleDays(`${yesterday.getFullYear()}-${m}-${d}`)).toBe(7);
  });

  it('accepts full timestamps by date prefix', () => {
    expect(priceStaleDays(`${todayStr()} 10:30:00`)).toBe(0);
  });
});

describe('dayChangePct', () => {
  it('computes relative change, null without a usable previous price', () => {
    expect(dayChangePct({ price: 110, prevPrice: 100 })).toBeCloseTo(10);
    expect(dayChangePct({ price: null, prevPrice: 100 })).toBeNull();
    expect(dayChangePct({ price: 110, prevPrice: null })).toBeNull();
    expect(dayChangePct({ price: 110, prevPrice: 0 })).toBeNull();
  });
});

describe('range52w', () => {
  it('returns null without history', () => {
    expect(range52w(null)).toBeNull();
    expect(range52w([])).toBeNull();
  });

  it('takes high/low over recent prices', () => {
    const today = new Date();
    const iso = (daysAgo: number) => {
      const d = new Date(today.getFullYear(), today.getMonth(), today.getDate() - daysAgo);
      const m = String(d.getMonth() + 1).padStart(2, '0');
      const day = String(d.getDate()).padStart(2, '0');
      return `${d.getFullYear()}-${m}-${day}`;
    };
    expect(
      range52w([
        { date: iso(10), price: 100 },
        { date: iso(20), price: 150 },
        { date: iso(30), price: 80 }
      ])
    ).toEqual({ high: 150, low: 80 });
  });
});

describe('toastPriceResult', () => {
  const price = { mnemonic: 'AAPL', price: 100, priceDate: '2024-01-01', currency: 'USD' };

  it('stays silent without a result or without prices', () => {
    toastPriceResult(null);
    toastPriceResult(undefined);
    toastPriceResult({ prices: [], errors: [] });
    expect(mockedToast.success).not.toHaveBeenCalled();
    expect(mockedToast.warning).not.toHaveBeenCalled();
  });

  it('warns with a fix hint for import flows', () => {
    toastPriceResult(
      { prices: [price], errors: [{ mnemonic: 'SAP', error: 'no symbol' }] },
      { fixHint: true }
    );
    expect(mockedToast.warning).toHaveBeenCalledWith(
      'Prices refreshed with 1 error — open Portfolio to fix symbols.'
    );
    expect(mockedToast.success).not.toHaveBeenCalled();
  });

  it('warns without the hint and confirms clean refreshes', () => {
    toastPriceResult({ prices: [], errors: [{ mnemonic: 'SAP', error: 'no symbol' }] });
    expect(mockedToast.warning).toHaveBeenCalledWith('Prices refreshed with 1 error');
    toastPriceResult({ prices: [price], errors: [] });
    expect(mockedToast.success).toHaveBeenCalledWith('Prices refreshed');
    toastPriceResult({ prices: [price], errors: [] }, { fixHint: true });
    expect(mockedToast.success).toHaveBeenCalledWith('Prices refreshed from Yahoo Finance');
  });
});
