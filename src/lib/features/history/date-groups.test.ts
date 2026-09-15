import { describe, expect, it } from 'vitest';
import { groupHistoryEntries, relativeDateGroup } from './date-groups';

// Wednesday, 16 September 2026, 14:30 local. This week runs Mon 14 – Sun 20;
// last week Mon 7 – Sun 13.
const NOW = new Date(2026, 8, 16, 14, 30);

const at = (year: number, month: number, date: number, hour = 12, minute = 0) =>
  new Date(year, month, date, hour, minute).toISOString();

describe('relativeDateGroup', () => {
  it('buckets today and yesterday by calendar day', () => {
    expect(relativeDateGroup(at(2026, 8, 16, 0, 0), NOW)).toBe('today');
    expect(relativeDateGroup(at(2026, 8, 15, 23, 59), NOW)).toBe('yesterday');
  });

  it('splits the rest of this week from last week', () => {
    expect(relativeDateGroup(at(2026, 8, 14, 0, 0), NOW)).toBe('thisWeek');
    expect(relativeDateGroup(at(2026, 8, 13, 23, 59), NOW)).toBe('lastWeek');
    expect(relativeDateGroup(at(2026, 8, 7, 0, 0), NOW)).toBe('lastWeek');
  });

  it('keeps earlier days of this month out of last week', () => {
    expect(relativeDateGroup(at(2026, 8, 6, 23, 59), NOW)).toBe('thisMonth');
    expect(relativeDateGroup(at(2026, 8, 1, 0, 0), NOW)).toBe('thisMonth');
  });

  it('buckets last month, earlier this year and older', () => {
    expect(relativeDateGroup(at(2026, 7, 31, 23, 59), NOW)).toBe('lastMonth');
    expect(relativeDateGroup(at(2026, 7, 1), NOW)).toBe('lastMonth');
    expect(relativeDateGroup(at(2026, 0, 1), NOW)).toBe('thisYear');
    expect(relativeDateGroup(at(2025, 11, 31, 23, 59), NOW)).toBe('older');
  });

  it('prefers yesterday over last week when the week flips', () => {
    const monday = new Date(2026, 8, 14, 9, 0);
    expect(relativeDateGroup(at(2026, 8, 13), monday)).toBe('yesterday');
  });
});

describe('groupHistoryEntries', () => {
  it('groups newest-first entries under relative date labels', () => {
    const entries = [
      { id: 4, occurredAt: at(2026, 8, 16, 9) },
      { id: 3, occurredAt: at(2026, 8, 16, 8) },
      { id: 2, occurredAt: at(2026, 8, 15, 9) },
      { id: 1, occurredAt: at(2026, 8, 10, 9) }
    ];
    const groups = groupHistoryEntries(entries, NOW);
    expect(groups.map((g) => g.key)).toEqual(['today', 'yesterday', 'lastWeek']);
    expect(groups.map((g) => g.label)).toEqual(['Today', 'Yesterday', 'Last week']);
    expect(groups[0].entries.map((e) => e.id)).toEqual([4, 3]);
    expect(groups[1].entries.map((e) => e.id)).toEqual([2]);
    expect(groups[2].entries.map((e) => e.id)).toEqual([1]);
  });

  it('returns no groups for an empty list', () => {
    expect(groupHistoryEntries([], NOW)).toEqual([]);
  });
});
