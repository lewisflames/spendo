import {
  parseRecentBooks,
  withRecentBook,
  type RecentBook
} from '$lib/features/welcome/welcome-utils';

const STORAGE_KEY = 'spendo-recent-books';

function load(): RecentBook[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw === null ? [] : parseRecentBooks(JSON.parse(raw));
  } catch {
    return [];
  }
}

/** Books opened on this machine, newest first, for the welcome screen. */
export const recentBooks = $state<{ value: RecentBook[] }>({ value: load() });

function persist(next: RecentBook[]) {
  recentBooks.value = next;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  } catch {
    // Storage full/blocked — the list stays memory-only.
  }
}

export function rememberBook(path: string) {
  persist(withRecentBook(recentBooks.value, path, Math.floor(Date.now() / 1000)));
}

export function forgetBook(path: string) {
  persist(recentBooks.value.filter((b) => b.path !== path));
}
