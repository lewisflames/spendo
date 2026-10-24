/**
 * One title and description per page, shared by the page header, the
 * book-less `BookGate` header, breadcrumbs, the sidebar and the palette so
 * they can never drift. Descriptions are plain and carry no trailing period.
 */
export const PAGE_META = {
  overview: { title: 'Overview', description: 'Your finances at a glance' },
  transactions: { title: 'Transactions', description: 'Everything recorded in this book' },
  accounts: { title: 'Accounts', description: 'Your account tree and balances' },
  reconcile: { title: 'Reconcile', description: 'Match your accounts to bank statements' },
  scheduled: { title: 'Scheduled', description: 'Recurring transactions' },
  budgets: { title: 'Budgets', description: 'Planned spending by year, leftovers roll over' },
  pnl: { title: 'Profit & loss', description: 'Income and expenses for a period' },
  balanceSheet: { title: 'Balance sheet', description: 'What you own and owe, as of today' },
  portfolio: { title: 'Portfolio', description: 'Investment holdings and performance' },
  realizedGains: { title: 'Realized gains', description: 'Sales and dividends by year' },
  settings: { title: 'Settings', description: 'App and book preferences' },
  about: { title: 'About Spendo', description: 'Version and updates' },
  importCsv: { title: 'Import CSV', description: 'Map columns, preview, then import' }
} as const satisfies Record<string, { title: string; description: string }>;

export type PageId = keyof typeof PAGE_META;
