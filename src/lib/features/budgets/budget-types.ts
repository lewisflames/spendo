import type { Budget, BudgetActualRow } from '$lib/tauri';

export interface BudgetRow {
  budget: Budget;
  actual: BudgetActualRow | undefined;
}

export interface BudgetGroup {
  category: string;
  rows: BudgetRow[];
}
