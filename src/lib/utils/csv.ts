function escapeCell(value: string): string {
  if (/[",\n\r]/.test(value)) {
    return `"${value.replaceAll('"', '""')}"`;
  }
  return value;
}

function toCsvText(lines: string[][]): string {
  return lines.map((r) => r.map(escapeCell).join(',')).join('\n');
}

function saveCsv(filename: string, text: string): void {
  const blob = new Blob(['\uFEFF' + text], { type: 'text/csv;charset=utf-8;' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  // WebKit (Tauri's macOS WebView) needs the blob URL alive past the click
  // task; revoking synchronously can abort the download.
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

export function downloadCsv(filename: string, headers: string[], rows: string[][]): void {
  saveCsv(filename, toCsvText([headers, ...rows]));
}

export interface ReportCsvOptions {
  /** Key/value preamble (period, currency…) rendered as `Key,Value` lines. */
  meta?: [string, string][];
  headers: string[];
  rows: string[][];
  /** Totals block after a blank line (same column shape as rows). */
  totals?: string[][];
}

/**
 * Report export: meta preamble + full data rows + totals. Numbers stay raw
 * (`String(n)`, machine-readable) — display formatting belongs in the UI.
 */
export function downloadReportCsv(filename: string, opts: ReportCsvOptions): void {
  const lines: string[][] = [];
  for (const [key, value] of opts.meta ?? []) lines.push([key, value]);
  if (lines.length > 0) lines.push([]);
  lines.push(opts.headers, ...opts.rows);
  if (opts.totals && opts.totals.length > 0) lines.push([], ...opts.totals);
  saveCsv(filename, toCsvText(lines));
}
