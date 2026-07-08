import type { ProfitLossRow } from '$lib/tauri';

export interface PnLTreeNode {
  row: ProfitLossRow;
  children: PnLTreeNode[];
}
