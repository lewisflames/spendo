/**
 * Locale helpers for CSV exports. The backend formats and writes the CSV
 * itself, but only the webview knows the locale’s numeric date order; derive
 * it once with `Intl.DateTimeFormat.formatToParts` and pass the parts +
 * separator to `export_transactions_csv`.
 */

export interface ExportDatePattern {
  parts: 'dmy' | 'mdy' | 'ymd';
  separator: string;
}

/**
 * Default filename offered in the save dialog, e.g.
 * `my-book-transactions-2026-09-20.csv`.
 */
export function exportFilename(
  kind: 'transactions' | 'accounts' | `realized-gains-${number}` | `tax-summary-${number}`,
  bookName: string,
  today: string
): string {
  const slug =
    bookName
      .trim()
      .replace(/\.spendo$/i, '')
      .replace(/[^\w.-]+/g, '-')
      .replace(/^-+|-+$/g, '') || 'spendo';
  return `${slug}-${kind}-${today}.csv`;
}

/**
 * Numeric date order + separator for a locale. Probes with 13 Dec 2001 so the
 * day/month order is unambiguous, using the same Intl options the exports used
 * before the backend took over date rendering.
 */
export function exportDatePattern(locale: string): ExportDatePattern {
  const fmt = new Intl.DateTimeFormat(locale || 'en-IE', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit'
  });
  const parts = fmt.formatToParts(new Date(2001, 11, 13));
  const order = parts
    .filter((p) => p.type === 'year' || p.type === 'month' || p.type === 'day')
    .map((p) => p.type[0])
    .join('');
  const literals = parts.filter((p) => p.type === 'literal').map((p) => p.value);
  const separator =
    literals.length > 0 && literals.every((l) => l === literals[0]) ? literals[0] : '/';
  const parts2: ExportDatePattern['parts'] =
    order === 'mdy' || order === 'ymd' || order === 'dmy' ? order : 'dmy';
  return { parts: parts2, separator };
}
