import { describe, expect, it } from 'vitest';
import type { Schedule } from '$lib/tauri';
import {
  frequencyLabel,
  isPostable,
  occurrenceLabel,
  scheduleStatus,
  scheduleToInput
} from './schedule-utils';

function schedule(partial: Partial<Schedule> = {}): Schedule {
  return {
    guid: 'sched-1',
    name: 'Rent',
    frequency: 'monthly',
    intervalMult: 1,
    startDate: '2024-01-01',
    endDate: null,
    maxOccurrences: null,
    occurrencesPosted: 0,
    nextDueDate: '2024-02-01',
    active: true,
    description: 'Monthly rent',
    num: null,
    notes: null,
    splits: [],
    previewAmount: 1000,
    createdAt: '2024-01-01T00:00:00Z',
    ...partial
  };
}

describe('frequencyLabel', () => {
  it('names presets at ×1', () => {
    expect(frequencyLabel('daily', 1)).toBe('Daily');
    expect(frequencyLabel('weekly', 1)).toBe('Weekly');
    expect(frequencyLabel('biweekly', 1)).toBe('Every 2 weeks');
    expect(frequencyLabel('monthly', 1)).toBe('Monthly');
    expect(frequencyLabel('quarterly', 1)).toBe('Quarterly');
    expect(frequencyLabel('yearly', 1)).toBe('Yearly');
  });

  it('builds every-N labels', () => {
    expect(frequencyLabel('daily', 3)).toBe('Every 3 days');
    expect(frequencyLabel('weekly', 2)).toBe('Every 2 weeks');
    expect(frequencyLabel('monthly', 1)).toBe('Monthly');
    expect(frequencyLabel('monthly', 6)).toBe('Every 6 months');
    expect(frequencyLabel('quarterly', 2)).toBe('Every 2 quarters');
    expect(frequencyLabel('yearly', 2)).toBe('Every 2 years');
    expect(frequencyLabel('biweekly', 2)).toBe('Every 4 weeks');
  });
});

describe('scheduleStatus', () => {
  it('prioritizes paused, then completed, then due comparison', () => {
    expect(scheduleStatus(schedule({ active: false }), '2024-05-01')).toBe('paused');
    expect(scheduleStatus(schedule({ nextDueDate: null }), '2024-05-01')).toBe('completed');
    expect(scheduleStatus(schedule({ nextDueDate: '2024-05-01' }), '2024-05-01')).toBe('overdue');
    expect(scheduleStatus(schedule({ nextDueDate: '2024-04-01' }), '2024-05-01')).toBe('overdue');
    expect(scheduleStatus(schedule({ nextDueDate: '2024-06-01' }), '2024-05-01')).toBe('upcoming');
  });
});

describe('isPostable', () => {
  it('allows due and overdue dates only', () => {
    expect(isPostable('2024-05-01', '2024-05-01')).toBe(true);
    expect(isPostable('2024-04-01', '2024-05-01')).toBe(true);
    expect(isPostable('2024-06-01', '2024-05-01')).toBe(false);
  });
});

describe('scheduleToInput', () => {
  it('round-trips editable fields', () => {
    const input = scheduleToInput(
      schedule({
        intervalMult: 2,
        endDate: '2024-12-31',
        maxOccurrences: 12,
        splits: [{ accountGuid: 'a', accountName: 'A', amount: 10, quantity: null, memo: null }]
      })
    );
    expect(input).toMatchObject({
      name: 'Rent',
      frequency: 'monthly',
      intervalMult: 2,
      startDate: '2024-01-01',
      endDate: '2024-12-31',
      maxOccurrences: 12,
      description: 'Monthly rent'
    });
    expect(input.splits).toEqual([{ accountGuid: 'a', amount: 10, quantity: null, memo: null }]);
  });
});

describe('occurrenceLabel', () => {
  it('shows cap progress, posted counts, or a dash', () => {
    expect(occurrenceLabel(schedule({ maxOccurrences: 12, occurrencesPosted: 3 }))).toBe('3 of 12');
    expect(occurrenceLabel(schedule({ occurrencesPosted: 3 }))).toBe('3 posted');
    expect(occurrenceLabel(schedule())).toBe('—');
  });
});
