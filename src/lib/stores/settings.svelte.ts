import { browser } from '$app/environment';

export type Locale = 'en-US' | 'en-GB' | 'en-IE' | 'de-DE' | 'fr-FR' | 'ja-JP' | 'pt-BR' | 'es-ES';
export type Theme = 'light' | 'dark' | 'system';
export type Density = 'comfortable' | 'compact';

export const ACCENT_IDS = ['zinc', 'blue', 'emerald', 'amber', 'rose', 'violet', 'custom'] as const;
export type AccentId = (typeof ACCENT_IDS)[number];

export interface AppSettings {
  locale: Locale;
  theme: Theme;
  accent: AccentId;
  customPrimary: string | null;
  pageSize: number;
  sidebarCollapsed: boolean;
  reduceMotion: boolean;
  density: Density;
}

const STORAGE_KEY = 'spendo-settings';

export const PAGE_SIZE_OPTIONS = [10, 25, 50, 100] as const;

const DEFAULT_SETTINGS: AppSettings = {
  locale: 'en-IE',
  theme: 'system',
  accent: 'zinc',
  customPrimary: null,
  pageSize: 25,
  sidebarCollapsed: false,
  reduceMotion: false,
  density: 'comfortable'
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

function isAccentId(v: unknown): v is AccentId {
  return typeof v === 'string' && (ACCENT_IDS as readonly string[]).includes(v);
}

function isHexColor(v: unknown): v is string {
  return typeof v === 'string' && /^#[0-9a-fA-F]{6}$/.test(v);
}

function isHexColorOrNull(v: unknown): v is string | null {
  return v === null || isHexColor(v);
}

/** Readable text color (near-white or near-black) on top of a custom hex primary. */
export function customPrimaryForeground(hex: string): string {
  const channel = (i: number) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  };
  const luminance = 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
  return luminance > 0.18 ? '#18181b' : '#fafafa';
}

/** Inline `<html>` overrides for a user-picked primary: rings, sidebar and chart-1 follow it. */
export function customAccentVars(hex: string): Record<string, string> {
  const foreground = customPrimaryForeground(hex);
  return {
    '--primary': hex,
    '--primary-foreground': foreground,
    '--ring': hex,
    '--chart-1': hex,
    '--sidebar-primary': hex,
    '--sidebar-primary-foreground': foreground,
    '--sidebar-ring': hex
  };
}

const CUSTOM_VAR_KEYS = Object.keys(customAccentVars('#000000'));

/** Reflect the accent layer on `<html>` (data attribute for presets, inline vars for custom). */
export function applyAccent(accent: AccentId, customPrimary: string | null): void {
  if (!browser) return;
  const root = document.documentElement;
  root.dataset.accent = accent;
  const vars =
    accent === 'custom' && isHexColor(customPrimary) ? customAccentVars(customPrimary) : null;
  if (vars) {
    for (const [key, value] of Object.entries(vars)) root.style.setProperty(key, value);
  } else {
    for (const key of CUSTOM_VAR_KEYS) root.style.removeProperty(key);
  }
}

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
    const isDensity = (v: unknown): v is Density => v === 'comfortable' || v === 'compact';
    return {
      locale: pick(raw.locale, DEFAULT_SETTINGS.locale, isLocale),
      theme: pick(raw.theme, DEFAULT_SETTINGS.theme, isTheme),
      accent: pick(raw.accent, DEFAULT_SETTINGS.accent, isAccentId),
      customPrimary: pick(raw.customPrimary, DEFAULT_SETTINGS.customPrimary, isHexColorOrNull),
      pageSize: pick(raw.pageSize, DEFAULT_SETTINGS.pageSize, isPageSize),
      sidebarCollapsed:
        typeof raw.sidebarCollapsed === 'boolean'
          ? raw.sidebarCollapsed
          : DEFAULT_SETTINGS.sidebarCollapsed,
      reduceMotion:
        typeof raw.reduceMotion === 'boolean' ? raw.reduceMotion : DEFAULT_SETTINGS.reduceMotion,
      density: pick(raw.density, DEFAULT_SETTINGS.density, isDensity)
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

/** Page-size change shared by every paginated list: persist + jump to page 1. */
export function applyPageSize(size: number, setPage: (page: number) => void): void {
  saveSettings({ pageSize: size });
  setPage(1);
}

/** Reflect the density layer on `<html>` (compact tightens page spacing). */
export function applyDensity(density: Density): void {
  if (!browser) return;
  document.documentElement.classList.toggle('compact', density === 'compact');
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
