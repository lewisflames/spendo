import SplitEditor from './split-editor.svelte';

export { SplitEditor };
export {
  autoFillIndex,
  draftFromSplit,
  formatDraftAmount,
  isBalanced,
  isInvestmentAccountType,
  INVESTMENT_ACCOUNT_TYPES,
  makeSplitDraft,
  parseAmount,
  remainderFor,
  splitBalance,
  splitErrors,
  splitToInput,
  splitTotals,
  type SplitDraft
} from './split-draft';
