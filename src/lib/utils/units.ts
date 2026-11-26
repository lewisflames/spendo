/**
 * Exact money amounts from the backend (`units.rs`): integer counts of
 * 1/`UNIT_SCALE` of a currency unit. Add and compare them as integers; turn
 * them into decimals only to format or chart them.
 */
export type Units = number & { readonly __brand: 'Units' };

/** Units per whole currency unit (`Units::SCALE` in Rust). */
export const UNIT_SCALE = 10_000;

export const ZERO_UNITS = 0 as Units;

/** Whole currency units, for formatting and charts. */
export function fromUnits(units: Units): number {
  return units / UNIT_SCALE;
}

/** Round a decimal amount to the nearest unit. */
export function toUnits(amount: number): Units {
  const units = Math.round(amount * UNIT_SCALE);
  return (Object.is(units, -0) ? 0 : units) as Units;
}

export function sumUnits(values: Iterable<Units>): Units {
  let total = 0;
  for (const v of values) total += v;
  return total as Units;
}

export function addUnits(a: Units, b: Units): Units {
  return (a + b) as Units;
}

export function subUnits(a: Units, b: Units): Units {
  return (a - b) as Units;
}

export function negUnits(a: Units): Units {
  return (a === 0 ? 0 : -a) as Units;
}

/**
 * Exact units for a plain decimal string (`-12.5`, `+0.1234`, `7`), read
 * digit by digit so no float rounding is involved. `null` for anything else,
 * including more than four decimals.
 */
export function decimalToUnits(text: string): Units | null {
  const match = /^([+-]?)(\d+)(?:\.(\d+))?$/.exec(text);
  if (!match) return null;
  const [, sign, whole, fraction = ''] = match;
  const digits = String(UNIT_SCALE).length - 1;
  if (fraction.length > digits) return null;
  const units = Number(whole) * UNIT_SCALE + Number(fraction.padEnd(digits, '0'));
  if (!Number.isSafeInteger(units)) return null;
  return (sign === '-' && units !== 0 ? -units : units) as Units;
}
