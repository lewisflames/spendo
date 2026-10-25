import budgetsDark from '$screenshots/budgets-dark.png';
import budgetsLight from '$screenshots/budgets-light.png';
import overviewDark from '$screenshots/overview-dark.png';
import overviewLight from '$screenshots/overview-light.png';
import portfolioDark from '$screenshots/portfolio-dark.png';
import portfolioLight from '$screenshots/portfolio-light.png';
import spendingDark from '$screenshots/spending-dark.png';
import spendingLight from '$screenshots/spending-light.png';
import transactionsDark from '$screenshots/transactions-dark.png';
import transactionsLight from '$screenshots/transactions-light.png';

/** One app screen in both themes, rendered from the demo book by `bun run screenshots`. */
export type Screenshot = { light: string; dark: string; alt: string };

export const SCREENSHOTS = {
  overview: {
    light: overviewLight,
    dark: overviewDark,
    alt: 'The Spendo overview: net worth, this month’s income and spending, and net worth over time'
  },
  transactions: {
    light: transactionsLight,
    dark: transactionsDark,
    alt: 'The transactions list with a daily activity heatmap, payees and tags'
  },
  spending: {
    light: spendingLight,
    dark: spendingDark,
    alt: 'The spending report: monthly spending stacked by category, with totals and changes'
  },
  budgets: {
    light: budgetsLight,
    dark: budgetsDark,
    alt: 'Yearly budgets with spent, remaining and pacing per category'
  },
  portfolio: {
    light: portfolioLight,
    dark: portfolioDark,
    alt: 'The portfolio: market value, total return, performance over time and allocation'
  }
} satisfies Record<string, Screenshot>;
