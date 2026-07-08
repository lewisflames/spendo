import { browser } from '$app/environment';

export type Locale = 'en-US' | 'en-GB' | 'de-DE' | 'fr-FR' | 'ja-JP' | 'pt-BR' | 'es-ES';
export type Theme = 'light' | 'dark' | 'system';

export interface AppSettings {
  locale: Locale;
  theme: Theme;
  pageSize: number;
  sidebarCollapsed: boolean;
  reduceMotion: boolean;
}

const STORAGE_KEY = 'spendo-settings';

export const PAGE_SIZE_OPTIONS = [10, 25, 50, 100] as const;

const DEFAULT_SETTINGS: AppSettings = {
  locale: 'en-US',
  theme: 'system',
  pageSize: 25,
  sidebarCollapsed: false,
  reduceMotion: false
};

function loadSettings(): AppSettings {
  if (!browser) return DEFAULT_SETTINGS;
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);
      return { ...DEFAULT_SETTINGS, ...parsed };
    }
  } catch {
    // ignore
  }
  return DEFAULT_SETTINGS;
}

export const appSettings = $state({ value: loadSettings() });

export function saveSettings(partial: Partial<AppSettings>) {
  appSettings.value = { ...appSettings.value, ...partial };
  if (!browser) return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(appSettings.value));
}

/** Restore every preference to its default (does not touch imported data). */
export function resetSettings() {
  saveSettings({ ...DEFAULT_SETTINGS });
}

/** Resolve the effective light/dark for the current OS preference. */
export function resolveTheme(theme: Theme): 'light' | 'dark' {
  if (theme !== 'system') return theme;
  if (!browser) return 'light';
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}
