import { getBookSettings, setBookSetting, type BookSettings } from '$lib/tauri';

export type { BookSettings };
export type BookSettingField = keyof BookSettings;

/** Backend snake_case keys for `set_book_setting`. */
const FIELD_TO_KEY: Record<BookSettingField, string> = {
  fiscalYearStartMonth: 'fiscal_year_start_month',
  defaultPnlPeriod: 'default_pnl_period',
  priceAutoRefresh: 'price_auto_refresh',
  exportCsvDelimiter: 'export_csv_delimiter',
  exportDateFormat: 'export_date_format',
  remindersLookaheadDays: 'reminders_lookahead_days'
};

export const DEFAULT_BOOK_SETTINGS: BookSettings = {
  fiscalYearStartMonth: 1,
  defaultPnlPeriod: 'last_12_months',
  priceAutoRefresh: 'off',
  exportCsvDelimiter: ',',
  exportDateFormat: 'iso',
  remindersLookaheadDays: 14
};

export const bookSettings = $state<{ value: BookSettings }>({
  value: { ...DEFAULT_BOOK_SETTINGS }
});

/** True while an initial load or save round-trip is in flight. */
export const bookSettingsLoading = $state<{ value: boolean }>({ value: false });

let loaded = false;
let inflight: Promise<BookSettings> | null = null;

/** Fetch the active book's settings (placeholder returns defaults). */
export function loadBookSettings(force = false): Promise<BookSettings> {
  if (inflight) return inflight;
  if (!force && loaded) return Promise.resolve(bookSettings.value);
  bookSettingsLoading.value = true;
  const p = getBookSettings()
    .then((settings) => {
      bookSettings.value = settings;
      loaded = true;
      return settings;
    })
    .catch((e) => {
      loaded = false;
      throw e;
    });
  inflight = p.finally(() => {
    if (inflight === p) inflight = null;
    bookSettingsLoading.value = false;
  });
  return inflight;
}

/** Persist one field and replace the store with the backend snapshot. */
export async function saveBookSetting<K extends BookSettingField>(
  field: K,
  value: BookSettings[K]
): Promise<BookSettings> {
  bookSettingsLoading.value = true;
  try {
    const updated = await setBookSetting(FIELD_TO_KEY[field], String(value));
    bookSettings.value = updated;
    loaded = true;
    return updated;
  } finally {
    bookSettingsLoading.value = false;
  }
}

/** Return to defaults when book-less or after Clear (empty book stays open). */
export function resetBookSettings() {
  bookSettings.value = { ...DEFAULT_BOOK_SETTINGS };
  loaded = false;
}
