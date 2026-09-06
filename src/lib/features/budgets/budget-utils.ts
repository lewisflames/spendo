import type { AccountNode, BudgetDateRange } from '$lib/tauri';
import { yearOf } from '$lib/utils/format.svelte';

export interface YearOption {
  value: string;
  label: string;
}

/** Build a contiguous year dropdown from an inclusive [min,max] range. */
export function buildYearOptions(minYear: number, maxYear: number): YearOption[] {
  return Array.from({ length: maxYear - minYear + 1 }, (_, i) => {
    const y = minYear + i;
    return { value: String(y), label: String(y) };
  });
}

/** Fallback dropdown when the book has no transaction dates yet. */
export function fallbackYearOptions(centerYear?: number, radius = 20): YearOption[] {
  const thisYear = centerYear ?? new Date().getFullYear();
  return Array.from({ length: radius + 6 }, (_, i) => {
    const y = thisYear - radius + i;
    return { value: String(y), label: String(y) };
  });
}

export function clampYear(y: number, minYear: number | null, maxYear: number | null): number {
  if (minYear !== null && y < minYear) return minYear;
  if (maxYear !== null && y > maxYear) return maxYear;
  return y;
}

/** Transaction-range bounds as years. `plusOne` extends max by a year for planning next year's budget. */
export function budgetYearBounds(
  dateRange: BudgetDateRange | null,
  plusOne = false
): { minYear: number | null; maxYear: number | null } {
  const minYear = dateRange?.earliestDate ? yearOf(dateRange.earliestDate) : null;
  const rawMax = dateRange?.latestDate ? yearOf(dateRange.latestDate) : null;
  const maxYear = rawMax !== null && plusOne ? rawMax + 1 : rawMax;
  return { minYear, maxYear };
}

export function flattenExpenseAccounts(
  nodes: AccountNode[]
): { guid: string; name: string; description: string | null }[] {
  const result: { guid: string; name: string; description: string | null }[] = [];
  function walk(list: AccountNode[]) {
    for (const n of list) {
      if (!n.placeholder && n.accountType === 'EXPENSE') {
        result.push({ guid: n.guid, name: n.name, description: n.description });
      }
      if (n.children.length > 0) walk(n.children);
    }
  }
  walk(nodes);
  return result;
}

/** Full display path per account guid, e.g. `['Expenses','Food']`. */
export function accountPathMap(trees: AccountNode[] | undefined): Record<string, string[]> {
  const map: Record<string, string[]> = {};
  if (!trees) return map;
  function walk(nodes: AccountNode[], ancestors: string[]) {
    for (const n of nodes) {
      const display = n.description ?? n.name;
      const path = [...ancestors, display];
      map[n.guid] = path;
      walk(n.children, path);
    }
  }
  walk(trees, []);
  return map;
}

/** Display name per account guid (description preferred). */
export function accountNameMap(trees: AccountNode[] | undefined): Record<string, string> {
  const map: Record<string, string> = {};
  if (!trees) return map;
  function walk(nodes: AccountNode[]) {
    for (const n of nodes) {
      map[n.guid] = n.description ?? n.name;
      walk(n.children);
    }
  }
  walk(trees);
  return map;
}
