import { goto } from '$app/navigation';
import { resolve } from '$app/paths';
import type { Component } from 'svelte';
import {
  ArrowLeft,
  ArrowLeftRight,
  ArrowRight,
  Briefcase,
  CircleHelp,
  FolderOpen,
  FolderPlus,
  FolderTree,
  History,
  Info,
  LogOut,
  PanelLeft,
  PiggyBank,
  Receipt,
  Save,
  Scale,
  Settings,
  SunMoon
} from '@lucide/svelte';
import { appSettings, resolveTheme, saveSettings } from '$lib/stores/settings.svelte';
import { keyboardShortcutsDialog } from '$lib/stores/ui.svelte';
import { activeBookPath, dbSession } from '$lib/stores/db.svelte';
import {
  closeActiveBook,
  createBookViaDialog,
  openBookViaDialog,
  saveBookAsViaDialog
} from '$lib/features/book/book-actions';
import { goBack, goForward } from '$lib/stores/nav-history.svelte';

type PaletteGroup = 'Go to' | 'Actions';

export interface PaletteCommand {
  id: string;
  title: string;
  group: PaletteGroup;
  icon: Component;
  keywords: string[];
  /** Hide the command when it would be a dead end (e.g. Close with no book). */
  when?: () => boolean;
  run: () => void | Promise<void>;
}

function goTo(
  path:
    | '/about'
    | '/accounts'
    | '/budgets'
    | '/history'
    | '/portfolio'
    | '/reconcile'
    | '/reports/pnl'
    | '/settings'
    | '/transactions'
): () => Promise<void> {
  return () => goto(resolve(path));
}

/** Static v1 command list: page navigation + app actions. No backend calls. */
export const PALETTE_COMMANDS: PaletteCommand[] = [
  {
    id: 'go-history',
    title: 'Go to History',
    group: 'Go to',
    icon: History,
    keywords: ['history', 'changelog', 'changes', 'undo', 'revert'],
    run: goTo('/history')
  },
  {
    id: 'go-accounts',
    title: 'Go to Accounts',
    group: 'Go to',
    icon: FolderTree,
    keywords: ['accounts', 'balance sheet', 'balances'],
    run: goTo('/accounts')
  },
  {
    id: 'go-transactions',
    title: 'Go to Transactions',
    group: 'Go to',
    icon: ArrowLeftRight,
    keywords: ['transactions', 'ledger', 'journal'],
    run: goTo('/transactions')
  },
  {
    id: 'go-pnl',
    title: 'Go to Profit & Loss',
    group: 'Go to',
    icon: Receipt,
    keywords: ['profit', 'loss', 'pnl', 'income', 'expenses', 'report'],
    run: goTo('/reports/pnl')
  },
  {
    id: 'go-reconcile',
    title: 'Go to Reconcile',
    group: 'Go to',
    icon: Scale,
    keywords: ['reconcile', 'reconciliation', 'bank statement', 'cleared'],
    run: goTo('/reconcile')
  },
  {
    id: 'go-budgets',
    title: 'Go to Budgets',
    group: 'Go to',
    icon: PiggyBank,
    keywords: ['budgets', 'budgeting', 'plan'],
    run: goTo('/budgets')
  },
  {
    id: 'go-portfolio',
    title: 'Go to Portfolio',
    group: 'Go to',
    icon: Briefcase,
    keywords: ['portfolio', 'investments', 'holdings', 'stocks'],
    run: goTo('/portfolio')
  },
  {
    id: 'go-settings',
    title: 'Go to Settings',
    group: 'Go to',
    icon: Settings,
    keywords: ['settings', 'preferences', 'export', 'import'],
    run: goTo('/settings')
  },
  {
    id: 'go-about',
    title: 'Go to About',
    group: 'Go to',
    icon: Info,
    keywords: ['about', 'version', 'help'],
    run: goTo('/about')
  },
  {
    id: 'toggle-theme',
    title: 'Toggle theme',
    group: 'Actions',
    icon: SunMoon,
    keywords: ['theme', 'dark', 'light', 'appearance'],
    run: () => {
      saveSettings({ theme: resolveTheme(appSettings.value.theme) === 'dark' ? 'light' : 'dark' });
    }
  },
  {
    id: 'toggle-sidebar',
    title: 'Toggle sidebar',
    group: 'Actions',
    icon: PanelLeft,
    keywords: ['sidebar', 'navigation', 'collapse'],
    run: () => {
      saveSettings({ sidebarCollapsed: !appSettings.value.sidebarCollapsed });
    }
  },
  {
    id: 'open-book',
    title: 'Open book…',
    group: 'Actions',
    icon: FolderOpen,
    keywords: ['open', 'book', 'database', 'spendo', 'switch'],
    run: () => {
      void openBookViaDialog();
    }
  },
  {
    id: 'new-book',
    title: 'New book…',
    group: 'Actions',
    icon: FolderPlus,
    keywords: ['new', 'create', 'book', 'database', 'spendo'],
    run: () => {
      void createBookViaDialog();
    }
  },
  {
    id: 'save-book-as',
    title: 'Save book as…',
    group: 'Actions',
    icon: Save,
    keywords: ['save', 'book', 'copy', 'export', 'spendo'],
    when: () => dbSession.value !== null,
    run: () => {
      void saveBookAsViaDialog();
    }
  },
  {
    id: 'close-book',
    title: 'Close book',
    group: 'Actions',
    icon: LogOut,
    keywords: ['close', 'book', 'database', 'switch'],
    when: () => activeBookPath.value !== null,
    run: () => {
      void closeActiveBook();
    }
  },
  {
    id: 'nav-back',
    title: 'Go back',
    group: 'Actions',
    icon: ArrowLeft,
    keywords: ['back', 'previous', 'history'],
    run: () => goBack()
  },
  {
    id: 'nav-forward',
    title: 'Go forward',
    group: 'Actions',
    icon: ArrowRight,
    keywords: ['forward', 'next', 'history'],
    run: () => goForward()
  },
  {
    id: 'open-shortcuts',
    title: 'Open keyboard shortcuts help',
    group: 'Actions',
    icon: CircleHelp,
    keywords: ['shortcuts', 'keys', 'help', 'cheatsheet'],
    run: () => {
      keyboardShortcutsDialog.value = true;
    }
  }
];

export const PALETTE_GROUPS: PaletteGroup[] = ['Go to', 'Actions'];
