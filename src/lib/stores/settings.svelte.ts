import { browser } from '$app/environment';

export type Locale = 'en-US' | 'en-GB' | 'en-IE' | 'de-DE' | 'fr-FR' | 'ja-JP' | 'pt-BR' | 'es-ES';
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
  locale: 'en-IE',
  theme: 'system',
  pageSize: 25,
  sidebarCollapsed: false,
  reduceMotion: false
};

const LOCALES: readonly string[] = [
  'en-US',
  'en-GB',
  'en-IE',
  'de-DE',
  'fr-FR',
  'ja-JP',
  'pt-BR',
  'es-ES'
];

function loadSettings(): AppSettings {
  if (!browser) return DEFAULT_SETTINGS;
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (!stored) return DEFAULT_SETTINGS;
    // Validate per key: hand-edited or corrupt storage must not inject values
    // that would throw in Intl.NumberFormat or break Select bindings.
    const parsed: unknown = JSON.parse(stored);
    if (typeof parsed !== 'object' || parsed === null) return DEFAULT_SETTINGS;
    const raw = parsed as Record<string, unknown>;
    const pick = <T>(value: unknown, fallback: T, valid: (v: unknown) => v is T): T =>
      valid(value) ? value : fallback;
    const isLocale = (v: unknown): v is Locale => typeof v === 'string' && LOCALES.includes(v);
    const isTheme = (v: unknown): v is Theme => v === 'light' || v === 'dark' || v === 'system';
    const isPageSize = (v: unknown): v is number =>
      PAGE_SIZE_OPTIONS.includes(v as (typeof PAGE_SIZE_OPTIONS)[number]);
    return {
      locale: pick(raw.locale, DEFAULT_SETTINGS.locale, isLocale),
      theme: pick(raw.theme, DEFAULT_SETTINGS.theme, isTheme),
      pageSize: pick(raw.pageSize, DEFAULT_SETTINGS.pageSize, isPageSize),
      sidebarCollapsed:
        typeof raw.sidebarCollapsed === 'boolean'
          ? raw.sidebarCollapsed
          : DEFAULT_SETTINGS.sidebarCollapsed,
      reduceMotion:
        typeof raw.reduceMotion === 'boolean' ? raw.reduceMotion : DEFAULT_SETTINGS.reduceMotion
    };
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
