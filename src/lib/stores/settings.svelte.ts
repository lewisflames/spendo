import { browser } from '$app/environment';

export type Locale = 'en-US' | 'de-DE' | 'fr-FR' | 'ja-JP' | 'pt-BR' | 'es-ES';
export type DashboardRange = '1M' | '3M' | '6M' | '1Y' | 'All';
export type FirstDayOfWeek = 0 | 1;
export interface AppSettings {
  locale: Locale;
  theme: 'light' | 'dark';
  pageSize: number;
  dashboardRange: DashboardRange;
  firstDayOfWeek: FirstDayOfWeek;
  priceSymbolOverrides: Record<string, string>;
}

const STORAGE_KEY = 'spendo-settings';

const DEFAULT_SETTINGS: AppSettings = {
  locale: 'en-US',
  theme: 'light',
  pageSize: 50,
  dashboardRange: '1Y',
  firstDayOfWeek: 1,
  priceSymbolOverrides: {}
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

export function resetSettings() {
  appSettings.value = { ...DEFAULT_SETTINGS };
  if (!browser) return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(appSettings.value));
}
