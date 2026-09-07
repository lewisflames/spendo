function escapeCell(value: string, delimiter: string): string {
  if (value.includes(delimiter) || /["\n\r]/.test(value)) {
    return `"${value.replaceAll('"', '""')}"`;
  }
  return value;
}

function toCsvText(lines: string[][], delimiter: string): string {
  return lines.map((r) => r.map((c) => escapeCell(c, delimiter)).join(delimiter)).join('\n');
}

/** BOM prefix the download and file-write sinks apply so Excel detects UTF-8. */
export const CSV_BOM = '﻿';

/** Pure CSV text builder (no BOM) shared by the download and file-write sinks. */
export function buildCsvText(
  headers: string[],
  rows: string[][],
  delimiter: ',' | ';' = ','
): string {
  return toCsvText([headers, ...rows], delimiter);
}

function saveCsv(filename: string, text: string): void {
  const blob = new Blob([CSV_BOM + text], { type: 'text/csv;charset=utf-8;' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  // WebKit (Tauri's macOS WebView) needs the blob URL alive past the click
  // task; revoking synchronously can abort the download.
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

export function downloadCsv(
  filename: string,
  headers: string[],
  rows: string[][],
  delimiter: ',' | ';' = ','
): void {
  saveCsv(filename, buildCsvText(headers, rows, delimiter));
}

export interface ReportCsvOptions {
  /** Key/value preamble (period, currency…) rendered as `Key,Value` lines. */
  meta?: [string, string][];
  headers: string[];
  rows: string[][];
  /** Totals block after a blank line (same column shape as rows). */
  totals?: string[][];
  delimiter?: ',' | ';';
}

/**
 * Report export: meta preamble + full data rows + totals. Numbers stay raw
 * (`String(n)`, machine-readable) — display formatting belongs in the UI.
 */
export function downloadReportCsv(filename: string, opts: ReportCsvOptions): void {
  const delimiter = opts.delimiter ?? ',';
  const lines: string[][] = [];
  for (const [key, value] of opts.meta ?? []) lines.push([key, value]);
  if (lines.length > 0) lines.push([]);
  lines.push(opts.headers, ...opts.rows);
  if (opts.totals && opts.totals.length > 0) lines.push([], ...opts.totals);
  saveCsv(filename, toCsvText(lines, delimiter));
}
