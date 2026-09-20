import { goto } from '$app/navigation';
import { resolve } from '$app/paths';
import type { Component } from 'svelte';
import {
  ArrowLeft,
  ArrowRight,
  CalendarPlus,
  CircleHelp,
  FileDown,
  FileSpreadsheet,
  FolderOpen,
  FolderPlus,
  Info,
  LogOut,
  PanelLeft,
  Plus,
  Save,
  Settings,
  SunMoon,
  Upload
} from '@lucide/svelte';
import { NAV_GROUPS, navHref } from '$lib/features/nav/nav-items';
import { navRoots } from '$lib/stores/nav.svelte';
import { appSettings, resolveTheme, saveSettings } from '$lib/stores/settings.svelte';
import { keyboardShortcutsDialog } from '$lib/stores/ui.svelte';
import { activeBookPath, dbSession } from '$lib/stores/db.svelte';
import {
  closeActiveBook,
  createBookViaDialog,
  openBookViaDialog,
  saveBookAsViaDialog
} from '$lib/features/book/book-actions';
import {
  exportAccountsCsvViaDialog,
  exportTransactionsCsvViaDialog,
  importGnucashViaDialog,
  openCsvImport
} from '$lib/features/book/import-export-actions';
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

/** Page navigation mirrors the sidebar config, so the two can never drift.
 * Dynamic targets (account roots) resolve at run time from the nav store. */
const goToCommands: PaletteCommand[] = NAV_GROUPS.flatMap((group) =>
  group.items.map((item): PaletteCommand => ({
    id: `go-${item.id}`,
    title: `Go to ${item.label}`,
    group: 'Go to',
    icon: item.icon,
    keywords: item.keywords,
    when: () => navHref(item, navRoots.value) !== null,
    run: () => {
      const href = navHref(item, navRoots.value);
      if (href) void goto(resolve(href));
    }
  }))
);

/** Static v1 command list: page navigation + app actions. No backend calls. */
export const PALETTE_COMMANDS: PaletteCommand[] = [
  ...goToCommands,
  {
    id: 'go-settings',
    title: 'Go to Settings',
    group: 'Go to',
    icon: Settings,
    keywords: ['settings', 'preferences'],
    run: () => goto(resolve('/settings'))
  },
  {
    id: 'go-about',
    title: 'Go to About',
    group: 'Go to',
    icon: Info,
    keywords: ['about', 'version', 'help'],
    run: () => goto(resolve('/about'))
  },
  {
    id: 'new-transaction',
    title: 'New transaction',
    group: 'Actions',
    icon: Plus,
    keywords: ['new', 'add', 'transaction', 'expense', 'income', 'spend'],
    when: () => dbSession.value !== null,
    run: () => goto(resolve('/transactions/new'))
  },
  {
    id: 'new-schedule',
    title: 'New scheduled transaction',
    group: 'Actions',
    icon: CalendarPlus,
    keywords: ['new', 'add', 'schedule', 'recurring', 'bill', 'subscription'],
    when: () => dbSession.value !== null,
    run: () => goto(resolve('/scheduled/new'))
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
    id: 'import-gnucash',
    title: 'Import GnuCash file…',
    group: 'Actions',
    icon: Upload,
    keywords: ['import', 'gnucash', 'merge', 'file'],
    when: () => dbSession.value !== null,
    run: () => {
      void importGnucashViaDialog();
    }
  },
  {
    id: 'import-csv',
    title: 'Import bank CSV…',
    group: 'Actions',
    icon: FileSpreadsheet,
    keywords: ['import', 'csv', 'bank statement', 'mapping', 'merge'],
    when: () => dbSession.value !== null,
    run: () => openCsvImport()
  },
  {
    id: 'export-transactions-csv',
    title: 'Export transactions CSV…',
    group: 'Actions',
    icon: FileDown,
    keywords: ['export', 'csv', 'transactions', 'download', 'interchange'],
    when: () => dbSession.value !== null,
    run: () => {
      void exportTransactionsCsvViaDialog();
    }
  },
  {
    id: 'export-accounts-csv',
    title: 'Export accounts CSV…',
    group: 'Actions',
    icon: FileDown,
    keywords: ['export', 'csv', 'accounts', 'download', 'interchange'],
    when: () => dbSession.value !== null,
    run: () => {
      void exportAccountsCsvViaDialog();
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
