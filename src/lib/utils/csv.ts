function escapeCell(value: string, delimiter: string): string {
  if (value.includes(delimiter) || /["\n\r]/.test(value)) {
    return `"${value.replaceAll('"', '""')}"`;
  }
  return value;
}

function toCsvText(lines: string[][], delimiter: string): string {
  return lines.map((r) => r.map((c) => escapeCell(c, delimiter)).join(delimiter)).join('\n');
}

/** Pure CSV text builder (no BOM) shared by all export writers. */
export function buildCsvText(
  headers: string[],
  rows: string[][],
  delimiter: ',' | ';' = ','
): string {
  return toCsvText([headers, ...rows], delimiter);
}
