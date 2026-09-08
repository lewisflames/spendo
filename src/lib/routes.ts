/**
 * Canonical page titles, keyed by base route path. Single source of truth for
 * the title-bar breadcrumb fallback; sidebar nav labels stay inline next to
 * their icons.
 */
export const ROUTE_TITLES: Record<string, string> = {
  '/accounts': 'Accounts',
  '/transactions': 'Transactions',
  '/history': 'History',
  '/scheduled': 'Scheduled',
  '/reconcile': 'Reconcile',
  '/settings': 'Settings',
  '/about': 'About',
  '/budgets': 'Budgets',
  '/reports/pnl': 'Profit & Loss',
  '/portfolio': 'Portfolio'
};

/** Title for any path: exact match first, then detail routes fall back to their section. */
export function routeTitle(pathname: string): string {
  // `/` is the welcome page and stays untitled in the breadcrumb.
  if (pathname === '/') return '';
  if (ROUTE_TITLES[pathname]) return ROUTE_TITLES[pathname];
  if (pathname.startsWith('/accounts/')) return ROUTE_TITLES['/accounts'];
  if (pathname.startsWith('/reconcile/')) return ROUTE_TITLES['/reconcile'];
  if (pathname.startsWith('/budgets/')) return ROUTE_TITLES['/budgets'];
  if (pathname.startsWith('/portfolio/')) return ROUTE_TITLES['/portfolio'];
  if (pathname.startsWith('/transactions/')) return ROUTE_TITLES['/transactions'];
  if (pathname.startsWith('/reports/')) return 'Reports';
  return 'Home';
}
