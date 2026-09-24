/* eslint-disable svelte/prefer-svelte-reactivity -- Date is read once for initial state, never stored reactively */
import type { BudgetDateRange } from '$lib/tauri';
import {
  budgetYearBounds,
  buildYearOptions,
  clampYear as clampYearToRange,
  fallbackYearOptions,
  type YearOption
} from '$lib/features/budgets/budget-utils';

interface BudgetYear {
  yearSel: string;
  /** Numeric view of the string selection. */
  year: number;
  minYear: number | null;
  maxYear: number | null;
  yearOptions: YearOption[];
  atMin: boolean;
  atMax: boolean;
  clampYear: (y: number) => number;
  /** Snap an out-of-range selection back into bounds (after range loads). */
  clampSelection: () => void;
  shiftYear: (delta: number) => void;
}

/**
 * Year selection shared by the budget list/detail pages and the add dialog:
 * a Select-friendly string selection with numeric view, bounds from the
 * book’s transaction window, and clamping/stepping helpers. `plusOne`
 * extends the max by a year so next year’s budget can be planned ahead.
 */
export function useBudgetYear(
  getDateRange: () => BudgetDateRange | null,
  opts: { plusOne?: boolean; fallbackCenter?: number; fallbackRadius?: number } = {}
): BudgetYear {
  let yearSel = $state(String(new Date().getFullYear()));

  const bounds = $derived.by(() => {
    const { minYear, maxYear: rawMax } = budgetYearBounds(getDateRange());
    return { minYear, maxYear: rawMax !== null && opts.plusOne ? rawMax + 1 : rawMax };
  });

  const yearOptions = $derived.by((): YearOption[] => {
    const { minYear, maxYear } = bounds;
    if (minYear === null || maxYear === null) {
      return fallbackYearOptions(opts.fallbackCenter, opts.fallbackRadius);
    }
    return buildYearOptions(minYear, maxYear);
  });

  function clampYear(y: number): number {
    return clampYearToRange(y, bounds.minYear, bounds.maxYear);
  }

  function clampSelection(): void {
    if (bounds.minYear === null || bounds.maxYear === null) return;
    const current = Number(yearSel);
    const y = clampYear(current);
    if (y !== current) yearSel = String(y);
  }

  return {
    get yearSel() {
      return yearSel;
    },
    set yearSel(v: string) {
      yearSel = v;
    },
    get year() {
      return Number(yearSel);
    },
    get minYear() {
      return bounds.minYear;
    },
    get maxYear() {
      return bounds.maxYear;
    },
    get yearOptions() {
      return yearOptions;
    },
    get atMin() {
      return bounds.minYear !== null && Number(yearSel) <= bounds.minYear;
    },
    get atMax() {
      return bounds.maxYear !== null && Number(yearSel) >= bounds.maxYear;
    },
    clampYear,
    clampSelection,
    shiftYear: (delta: number) => {
      yearSel = String(clampYear(Number(yearSel) + delta));
    }
  };
}
