import { describe, expect, it } from 'vitest';
import type { PortfolioHolding } from '$lib/tauri';
import { computeRebalance } from './rebalance';

function holding(
  commodityGuid: string,
  marketValue: number,
  overrides: Partial<PortfolioHolding> = {}
): PortfolioHolding {
  return {
    commodityGuid,
    mnemonic: commodityGuid.toUpperCase(),
    fullName: null,
    namespace: 'NASDAQ',
    shares: marketValue / 10,
    costBasis: marketValue,
    openCostBasis: marketValue,
    marketValue,
    avgCostPerShare: 10,
    gainLoss: 0,
    unrealizedGain: 0,
    realizedGain: 0,
    dividends: 0,
    totalReturn: 0,
    basisWarning: null,
    xirr: null,
    allocationPct: 0,
    price: 10,
    prevPrice: null,
    priceDate: null,
    accounts: [
      {
        accountGuid: `${commodityGuid}-account`,
        accountName: 'Broker',
        accountDescription: null,
        accountCode: null,
        shares: marketValue / 10,
        costBasis: marketValue
      }
    ],
    ...overrides
  };
}

const targets = (...entries: [string, number][]) => new Map(entries);

describe('computeRebalance', () => {
  it('tops up the underweight holding to reach equal targets', () => {
    const result = computeRebalance(
      [holding('a', 750), holding('b', 250)],
      targets(['a', 50], ['b', 50])
    );

    // Post-contribution basis 1500 (750 / 50%): add 500 to b only.
    expect(result.cashNeeded).toBe(500);
    expect(result.cashInvested).toBe(500);
    expect(result.cashReserve).toBe(0);
    expect(result.rows[0].buyAmount).toBe(0);
    expect(result.rows[0].targetValue).toBe(750);
    expect(result.rows[1].buyAmount).toBe(500);
    expect(result.rows[1].targetValue).toBe(750);
    expect(result.atTarget).toBe(false);
  });

  it('asks for enough cash to dilute an overweight holding', () => {
    const result = computeRebalance(
      [holding('a', 900), holding('b', 100)],
      targets(['a', 50], ['b', 50])
    );

    // a needs a 1800 basis (900 / 50%), so 800 of new cash goes to b.
    expect(result.cashNeeded).toBe(800);
    expect(result.rows[0].buyAmount).toBe(0);
    expect(result.rows[1].buyAmount).toBe(800);
    expect(result.rows[1].targetValue).toBe(900);
  });

  it('respects uneven targets', () => {
    const result = computeRebalance(
      [holding('a', 900), holding('b', 100)],
      targets(['a', 60], ['b', 40])
    );

    // Basis 1500 (900 / 60%): a is at target, b needs 500.
    expect(result.cashNeeded).toBe(500);
    expect(result.rows[0].targetValue).toBe(900);
    expect(result.rows[0].buyAmount).toBe(0);
    expect(result.rows[1].targetValue).toBe(600);
    expect(result.rows[1].buyAmount).toBe(500);
  });

  it('reports when no cash is needed', () => {
    const result = computeRebalance(
      [holding('a', 500), holding('b', 500)],
      targets(['a', 50], ['b', 50])
    );

    expect(result.cashNeeded).toBe(0);
    expect(result.cashInvested).toBe(0);
    expect(result.atTarget).toBe(true);
    expect(result.rows.every((r) => r.buyAmount === 0)).toBe(true);
  });

  it('keeps the unallocated share of targets below 100% as cash', () => {
    const result = computeRebalance(
      [holding('a', 600), holding('b', 100)],
      targets(['a', 30], ['b', 30])
    );

    // Basis 2000 (600 / 30%): 500 buys b, 800 stays as the unallocated 40%.
    expect(result.cashNeeded).toBe(1300);
    expect(result.cashInvested).toBe(500);
    expect(result.cashReserve).toBe(800);
    expect(result.rows[0].buyAmount).toBe(0);
    expect(result.rows[1].buyAmount).toBe(500);
  });

  it('leaves holdings without a target outside the calculation', () => {
    const result = computeRebalance(
      [holding('a', 1000), holding('b', 200), holding('c', 200)],
      targets(['b', 50], ['c', 50])
    );

    // b and c already split the targeted value 50/50; a's value is ignored.
    expect(result.cashNeeded).toBe(0);
    expect(result.unallocatedCount).toBe(1);
    expect(result.rows[0].targetValue).toBe(0);
    expect(result.rows[0].buyAmount).toBe(0);
    expect(result.atTarget).toBe(true);
  });

  it('estimates the shares to buy at the latest price', () => {
    const result = computeRebalance(
      [holding('a', 750), holding('b', 250)],
      targets(['a', 50], ['b', 50])
    );

    expect(result.rows[1].buyShares).toBe(50);
    expect(result.rows[0].buyShares).toBe(0);
  });

  it('needs no cash when a single holding already matches its target', () => {
    const result = computeRebalance([holding('a', 1000)], targets(['a', 100]));

    expect(result.cashNeeded).toBe(0);
    expect(result.atTarget).toBe(true);
  });

  it('reports all holdings as unallocated when no target is set', () => {
    const result = computeRebalance([holding('a', 1000), holding('b', 500)], targets());

    expect(result.targetSumPct).toBe(0);
    expect(result.unallocatedCount).toBe(2);
    expect(result.cashNeeded).toBe(0);
    expect(result.atTarget).toBe(false);
  });

  it('excludes watch-only and unpriced holdings', () => {
    const result = computeRebalance(
      [
        holding('a', 1000),
        holding('w', 0, { accounts: [], shares: 0 }),
        holding('u', 500, { price: null })
      ],
      targets(['a', 100], ['w', 50], ['u', 50])
    );

    expect(result.rows.map((r) => r.commodityGuid)).toEqual(['a']);
    expect(result.excluded.map((e) => [e.commodityGuid, e.reason])).toEqual([
      ['w', 'watch_only'],
      ['u', 'unpriced']
    ]);
    expect(result.targetSumPct).toBe(100);
    expect(result.invalid).toBe(false);
    expect(result.unallocatedCount).toBe(0);
  });

  it('flags targets above 100% in total', () => {
    const result = computeRebalance(
      [holding('a', 500), holding('b', 500)],
      targets(['a', 60], ['b', 50])
    );

    expect(result.invalid).toBe(true);
  });

  it('flags negative and non-finite targets', () => {
    const negative = computeRebalance([holding('a', 500)], targets(['a', -1]));
    const nan = computeRebalance([holding('a', 500)], targets(['a', Number.NaN]));

    expect(negative.invalid).toBe(true);
    expect(nan.invalid).toBe(true);
  });

  it('returns an empty plan for an empty portfolio', () => {
    const result = computeRebalance([], targets(['a', 100]));

    expect(result.rows).toEqual([]);
    expect(result.cashNeeded).toBe(0);
    expect(result.unallocatedCount).toBe(0);
    expect(result.targetSumPct).toBe(0);
  });
});
