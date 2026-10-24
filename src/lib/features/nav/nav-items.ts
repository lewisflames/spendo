import type { Pathname } from '$app/types';
import type { Component } from 'svelte';
import {
  ArrowLeftRight,
  Briefcase,
  ChartColumnStacked,
  CalendarClock,
  HandCoins,
  Landmark,
  LayoutDashboard,
  PiggyBank,
  Receipt,
  Scale,
  ScrollText
} from '@lucide/svelte';
import { PAGE_META } from './page-meta';

export type NavBadgeKey = 'reconcile' | 'scheduled';

/** `g`-chord targets for the app’s primary pages. */
export const NAV_CHORD_TARGETS = {
  h: '/',
  t: '/transactions',
  c: '/accounts',
  r: '/reconcile',
  a: '/reports/balance-sheet',
  s: '/scheduled',
  b: '/budgets',
  p: '/reports/pnl',
  e: '/reports/spending',
  i: '/portfolio'
} as const;

export type NavChordKey = keyof typeof NAV_CHORD_TARGETS;

export interface NavItem {
  id: string;
  label: string;
  icon: Component;
  /** Palette / global search keywords. */
  keywords: string[];
  href: Pathname;
  /** `g`-chord key, shown in collapsed tooltips and the shortcuts help. */
  chord?: NavChordKey;
  match: (pathname: string) => boolean;
  badge?: NavBadgeKey;
}

export interface NavGroup {
  label: string;
  items: NavItem[];
}

export const NAV_GROUPS: NavGroup[] = [
  {
    label: 'Ledger',
    items: [
      {
        id: 'overview',
        label: PAGE_META.overview.title,
        icon: LayoutDashboard,
        keywords: ['overview', 'home', 'dashboard', 'summary', 'net worth'],
        href: '/',
        chord: 'h',
        match: (pathname) => pathname === '/'
      },
      {
        id: 'transactions',
        label: PAGE_META.transactions.title,
        icon: ArrowLeftRight,
        keywords: ['transactions', 'ledger', 'journal'],
        href: '/transactions',
        chord: 't',
        match: (pathname) =>
          pathname === '/transactions' ||
          pathname.startsWith('/transactions/') ||
          pathname.startsWith('/import/')
      },
      {
        id: 'accounts',
        label: PAGE_META.accounts.title,
        icon: Landmark,
        keywords: ['accounts', 'assets', 'liabilities', 'equity', 'income', 'expenses', 'bank'],
        href: '/accounts',
        chord: 'c',
        match: (pathname) => pathname === '/accounts' || pathname.startsWith('/accounts/')
      },
      {
        id: 'reconcile',
        label: PAGE_META.reconcile.title,
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
    label: 'Planning',
    items: [
      {
        id: 'scheduled',
        label: PAGE_META.scheduled.title,
        icon: CalendarClock,
        keywords: ['scheduled', 'recurring', 'bills', 'subscriptions'],
        href: '/scheduled',
        chord: 's',
        badge: 'scheduled',
        match: (pathname) => pathname === '/scheduled' || pathname.startsWith('/scheduled/')
      },
      {
        id: 'budgets',
        label: PAGE_META.budgets.title,
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
        label: PAGE_META.pnl.title,
        icon: Receipt,
        keywords: ['profit', 'loss', 'pnl', 'income', 'expenses', 'report'],
        href: '/reports/pnl',
        chord: 'p',
        match: (pathname) => pathname === '/reports/pnl'
      },
      {
        id: 'spending',
        label: PAGE_META.spending.title,
        icon: ChartColumnStacked,
        keywords: ['spending', 'expenses', 'categories', 'where money goes', 'report'],
        href: '/reports/spending',
        chord: 'e',
        match: (pathname) => pathname === '/reports/spending'
      },
      {
        id: 'balance-sheet',
        label: PAGE_META.balanceSheet.title,
        icon: ScrollText,
        keywords: ['balance sheet', 'accounts', 'assets', 'liabilities', 'equity', 'net worth'],
        href: '/reports/balance-sheet',
        chord: 'a',
        match: (pathname) => pathname === '/reports/balance-sheet'
      }
    ]
  },
  {
    label: 'Investments',
    items: [
      {
        id: 'portfolio',
        label: PAGE_META.portfolio.title,
        icon: Briefcase,
        keywords: ['portfolio', 'investments', 'holdings', 'stocks'],
        href: '/portfolio',
        chord: 'i',
        match: (pathname) =>
          pathname === '/portfolio' ||
          (pathname.startsWith('/portfolio/') && pathname !== '/portfolio/gains')
      },
      {
        id: 'realized-gains',
        label: PAGE_META.realizedGains.title,
        icon: HandCoins,
        keywords: ['realized', 'gains', 'capital gains', 'sales', 'dividends', 'tax'],
        href: '/portfolio/gains',
        match: (pathname) => pathname === '/portfolio/gains'
      }
    ]
  }
];
