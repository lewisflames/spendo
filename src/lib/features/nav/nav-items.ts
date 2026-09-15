import type { Pathname } from '$app/types';
import type { Component } from 'svelte';
import {
  ArrowLeftRight,
  Briefcase,
  CalendarClock,
  Coins,
  CreditCard,
  FolderTree,
  Landmark,
  PiggyBank,
  Receipt,
  Scale,
  TrendingDown,
  TrendingUp
} from '@lucide/svelte';
import type { NavRoot } from '$lib/tauri';

export type NavBadgeKey = 'reconcile' | 'scheduled';

/** `g`-chord targets for the app's primary pages. */
export const NAV_CHORD_TARGETS = {
  t: '/transactions',
  r: '/reconcile',
  a: '/accounts',
  s: '/scheduled',
  b: '/budgets',
  p: '/reports/pnl',
  i: '/portfolio'
} as const;

export type NavChordKey = keyof typeof NAV_CHORD_TARGETS;

export interface NavItem {
  id: string;
  label: string;
  icon: Component;
  /** Palette / global search keywords. */
  keywords: string[];
  /** Static route, or a resolver over the fetched account roots (`null` hides the item). */
  href: Pathname | ((roots: NavRoot[]) => Pathname | null);
  /** `g`-chord key, shown in collapsed tooltips and the shortcuts help. */
  chord?: NavChordKey;
  match: (pathname: string, roots: NavRoot[]) => boolean;
  badge?: NavBadgeKey;
}

export interface NavGroup {
  label: string;
  items: NavItem[];
}

function rootGuid(roots: NavRoot[], category: string): string | null {
  return roots.find((root) => root.category === category)?.guid ?? null;
}

function rootHref(roots: NavRoot[], category: string): Pathname | null {
  const guid = rootGuid(roots, category);
  return guid ? `/accounts/${guid}` : null;
}

function rootMatch(pathname: string, roots: NavRoot[], category: string): boolean {
  const guid = rootGuid(roots, category);
  return guid !== null && pathname === `/accounts/${guid}`;
}

export const NAV_GROUPS: NavGroup[] = [
  {
    label: 'Ledger',
    items: [
      {
        id: 'transactions',
        label: 'Transactions',
        icon: ArrowLeftRight,
        keywords: ['transactions', 'ledger', 'journal'],
        href: '/transactions',
        chord: 't',
        match: (pathname) => pathname === '/transactions' || pathname.startsWith('/transactions/')
      },
      {
        id: 'reconcile',
        label: 'Reconcile',
        icon: Scale,
        keywords: ['reconcile', 'reconciliation', 'bank statement', 'cleared'],
        href: '/reconcile',
        chord: 'r',
        badge: 'reconcile',
        match: (pathname) => pathname === '/reconcile' || pathname.startsWith('/reconcile/')
      }
    ]
  },
  {
    label: 'Accounts',
    items: [
      {
        id: 'all-accounts',
        label: 'All accounts',
        icon: FolderTree,
        keywords: ['accounts', 'balance sheet', 'balances', 'overview'],
        href: '/accounts',
        chord: 'a',
        match: (pathname) => pathname === '/accounts'
      },
      {
        id: 'assets',
        label: 'Assets',
        icon: Landmark,
        keywords: ['assets', 'bank', 'cash', 'investments'],
        href: (roots) => rootHref(roots, 'ASSET'),
        match: (pathname, roots) => rootMatch(pathname, roots, 'ASSET')
      },
      {
        id: 'liabilities',
        label: 'Liabilities',
        icon: CreditCard,
        keywords: ['liabilities', 'debt', 'credit card', 'loans'],
        href: (roots) => rootHref(roots, 'LIABILITY'),
        match: (pathname, roots) => rootMatch(pathname, roots, 'LIABILITY')
      },
      {
        id: 'equity',
        label: 'Equity',
        icon: Coins,
        keywords: ['equity', 'opening balances', 'net worth'],
        href: (roots) => rootHref(roots, 'EQUITY'),
        match: (pathname, roots) => rootMatch(pathname, roots, 'EQUITY')
      },
      {
        id: 'income',
        label: 'Income',
        icon: TrendingUp,
        keywords: ['income', 'revenue', 'salary'],
        href: (roots) => rootHref(roots, 'INCOME'),
        match: (pathname, roots) => rootMatch(pathname, roots, 'INCOME')
      },
      {
        id: 'expenses',
        label: 'Expenses',
        icon: TrendingDown,
        keywords: ['expenses', 'spending', 'costs'],
        href: (roots) => rootHref(roots, 'EXPENSE'),
        match: (pathname, roots) => rootMatch(pathname, roots, 'EXPENSE')
      }
    ]
  },
  {
    label: 'Planning',
    items: [
      {
        id: 'scheduled',
        label: 'Scheduled',
        icon: CalendarClock,
        keywords: ['scheduled', 'recurring', 'bills', 'subscriptions'],
        href: '/scheduled',
        chord: 's',
        badge: 'scheduled',
        match: (pathname) => pathname === '/scheduled' || pathname.startsWith('/scheduled/')
      },
      {
        id: 'budgets',
        label: 'Budgets',
        icon: PiggyBank,
        keywords: ['budgets', 'budgeting', 'plan'],
        href: '/budgets',
        chord: 'b',
        match: (pathname) => pathname === '/budgets' || pathname.startsWith('/budgets/')
      }
    ]
  },
  {
    label: 'Reports',
    items: [
      {
        id: 'pnl',
        label: 'Profit & Loss',
        icon: Receipt,
        keywords: ['profit', 'loss', 'pnl', 'income', 'expenses', 'report'],
        href: '/reports/pnl',
        chord: 'p',
        match: (pathname) => pathname === '/reports/pnl'
      },
      {
        id: 'portfolio',
        label: 'Portfolio',
        icon: Briefcase,
        keywords: ['portfolio', 'investments', 'holdings', 'stocks'],
        href: '/portfolio',
        chord: 'i',
        match: (pathname) => pathname === '/portfolio' || pathname.startsWith('/portfolio/')
      }
    ]
  }
];

/** Resolved href for an item (`null` = not navigable yet / missing root). */
export function navHref(item: NavItem, roots: NavRoot[]): Pathname | null {
  return typeof item.href === 'function' ? item.href(roots) : item.href;
}

export function navItemActive(item: NavItem, pathname: string, roots: NavRoot[]): boolean {
  return item.match(pathname, roots);
}
