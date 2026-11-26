import { describe, expect, it } from 'vitest';
import { decimalToUnits, fromUnits, negUnits, sumUnits, toUnits } from './units';

describe('units', () => {
  it('rounds decimals to the nearest unit without a negative zero', () => {
    expect(toUnits(12.34)).toBe(123_400);
    expect(toUnits(0.00004)).toBe(0);
    expect(Object.is(toUnits(-0.00004), -0)).toBe(false);
    expect(fromUnits(toUnits(-12.5))).toBe(-12.5);
  });

  it('sums cents exactly', () => {
    expect(sumUnits([toUnits(0.1), toUnits(0.2)])).toBe(toUnits(0.3));
    expect(Object.is(negUnits(toUnits(0)), -0)).toBe(false);
  });

  it('parses plain decimals digit by digit', () => {
    expect(decimalToUnits('7')).toBe(70_000);
    expect(decimalToUnits('-12.5')).toBe(-125_000);
    expect(decimalToUnits('+0.1234')).toBe(1_234);
    expect(Object.is(decimalToUnits('-0.00'), -0)).toBe(false);
    expect(decimalToUnits('0.12345')).toBeNull();
    expect(decimalToUnits('1e3')).toBeNull();
    expect(decimalToUnits('.5')).toBeNull();
    expect(decimalToUnits('')).toBeNull();
    expect(decimalToUnits('9'.repeat(20))).toBeNull();
  });
});
