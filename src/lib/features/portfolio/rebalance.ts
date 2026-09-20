import type { PortfolioHolding } from '$lib/tauri';

/**
 * Rebalance calculator math: given per-holding targets, how much cash to put
 * into each holding to reach them without selling anything.
 *
 * Only holdings with a target take part in the calculation. Their ratios can
 * be reached by buying; untargeted value would need selling to dilute, so it
 * stays outside the plan (the card lists it as "without a target").
 */

export interface RebalanceRow {
  commodityGuid: string;
  mnemonic: string;
  fullName: string | null;
  price: number | null;
  currentValue: number;
  targetPct: number;
  /** Post-contribution value that matches the target (0 without a target). */
  targetValue: number;
  /** Cash going into this holding (0 without a target). */
  buyAmount: number;
  buyShares: number | null;
}

export interface ExcludedHolding {
  commodityGuid: string;
  mnemonic: string;
  reason: 'watch_only' | 'unpriced';
}

export interface RebalanceResult {
  rows: RebalanceRow[];
  excluded: ExcludedHolding[];
  /** Cash contribution that reaches every target (buys plus any reserve). */
  cashNeeded: number;
  /** Part of `cashNeeded` that actually goes into holdings. */
  cashInvested: number;
  /** Share of the contribution that stays as cash (targets below 100%). */
  cashReserve: number;
  targetSumPct: number;
  /** Eligible holdings left without a target (outside the calculation). */
  unallocatedCount: number;
  /** Targets outside 0–100% or summing to more than 100%. */
  invalid: boolean;
  /** Targets already match the holdings: no cash needed. */
  atTarget: boolean;
}

/** Money below this is display dust and counts as "no buy". */
export const TRADE_DUST = 0.005;
const EPSILON = TRADE_DUST;
/** Percentage-point slack for the target sum (rounding dust). */
const PCT_EPSILON = 1e-6;

/**
 * Cash needed per holding. Eligible holdings have an investment account
 * (watch-only commodities are excluded) and a usable price (the backend values
 * unpriced holdings at 0, so including them would suggest phantom buys).
 */
export function computeRebalance(
  holdings: PortfolioHolding[],
  targets: ReadonlyMap<string, number>
): RebalanceResult {
  const excluded: ExcludedHolding[] = [];
  const included: PortfolioHolding[] = [];
  for (const h of holdings) {
    if (h.accounts.length === 0) {
      excluded.push({ commodityGuid: h.commodityGuid, mnemonic: h.mnemonic, reason: 'watch_only' });
    } else if (h.price === null || h.price <= 0) {
      excluded.push({ commodityGuid: h.commodityGuid, mnemonic: h.mnemonic, reason: 'unpriced' });
    } else {
      included.push(h);
    }
  }

  const targetOf = (guid: string): number => {
    const raw = targets.get(guid) ?? 0;
    return Number.isFinite(raw) ? Math.max(0, raw) : 0;
  };

  let invalid = included.some((h) => {
    const raw = targets.get(h.commodityGuid) ?? 0;
    return !Number.isFinite(raw) || raw < 0;
  });
  const targetSumPct = included.reduce((sum, h) => sum + targetOf(h.commodityGuid), 0);
  if (targetSumPct > 100 + PCT_EPSILON) invalid = true;

  // Only targeted holdings take part: their ratios can be reached by buying.
  // Untargeted value would need selling to dilute, so it stays outside.
  const allocated = included.filter((h) => targetOf(h.commodityGuid) > 0);
  const allocatedValue = allocated.reduce((sum, h) => sum + h.marketValue, 0);

  // Smallest post-contribution basis at which every allocated holding sits at
  // or below its target: the most overweight holding sets the cash needed.
  // (Buying a fixed amount keeps everyone else at or below target too, so a
  // single top-up reaches every ratio exactly.)
  let basis = allocatedValue;
  for (const h of allocated) {
    basis = Math.max(basis, h.marketValue / (targetOf(h.commodityGuid) / 100));
  }

  const rows: RebalanceRow[] = included.map((h) => {
    const targetPct = targetOf(h.commodityGuid);
    if (targetPct <= 0) {
      return {
        commodityGuid: h.commodityGuid,
        mnemonic: h.mnemonic,
        fullName: h.fullName,
        price: h.price,
        currentValue: h.marketValue,
        targetPct,
        targetValue: 0,
        buyAmount: 0,
        buyShares: null
      };
    }
    const targetValue = (basis * targetPct) / 100;
    const buyAmount = Math.max(0, targetValue - h.marketValue);
    return {
      commodityGuid: h.commodityGuid,
      mnemonic: h.mnemonic,
      fullName: h.fullName,
      price: h.price,
      currentValue: h.marketValue,
      targetPct,
      targetValue,
      buyAmount,
      buyShares: h.price !== null && h.price > 0 ? buyAmount / h.price : null
    };
  });

  const cashNeeded = Math.max(0, basis - allocatedValue);
  const cashInvested = rows.reduce((sum, r) => sum + r.buyAmount, 0);

  return {
    rows,
    excluded,
    cashNeeded,
    cashInvested,
    cashReserve: Math.max(0, cashNeeded - cashInvested),
    targetSumPct,
    unallocatedCount: included.length - allocated.length,
    invalid,
    atTarget: allocated.length > 0 && cashNeeded < EPSILON
  };
}
