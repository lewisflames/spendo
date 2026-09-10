import type { Schedule, ScheduleFrequency } from '$lib/tauri';

export const FREQUENCY_OPTIONS: { value: ScheduleFrequency; label: string }[] = [
  { value: 'daily', label: 'Daily' },
  { value: 'weekly', label: 'Weekly' },
  { value: 'biweekly', label: 'Every 2 weeks' },
  { value: 'monthly', label: 'Monthly' },
  { value: 'quarterly', label: 'Quarterly' },
  { value: 'yearly', label: 'Yearly' }
];

const SINGULAR: Record<ScheduleFrequency, string> = {
  daily: 'day',
  weekly: 'week',
  biweekly: '2 weeks',
  monthly: 'month',
  quarterly: 'quarter',
  yearly: 'year'
};

const PLURAL: Record<ScheduleFrequency, string> = {
  daily: 'days',
  weekly: 'weeks',
  biweekly: '2 weeks',
  monthly: 'months',
  quarterly: 'quarters',
  yearly: 'years'
};

/**
 * Human label for a recurrence rule: the preset name at ×1 ("Monthly",
 * "Every 2 weeks"), otherwise "Every N <units>" ("Every 3 months").
 */
export function frequencyLabel(frequency: ScheduleFrequency, intervalMult: number): string {
  if (intervalMult <= 1) {
    switch (frequency) {
      case 'daily':
        return 'Daily';
      case 'weekly':
        return 'Weekly';
      case 'biweekly':
        return 'Every 2 weeks';
      case 'monthly':
        return 'Monthly';
      case 'quarterly':
        return 'Quarterly';
      case 'yearly':
        return 'Yearly';
    }
  }
  if (frequency === 'biweekly') return `Every ${2 * intervalMult} weeks`;
  return `Every ${intervalMult} ${intervalMult === 1 ? SINGULAR[frequency] : PLURAL[frequency]}`;
}

export type ScheduleStatus = 'paused' | 'completed' | 'overdue' | 'upcoming';

/**
 * Card/table status for a schedule against `today` (YYYY-MM-DD, lexicographic
 * comparison is safe for zero-padded ISO dates).
 */
export function scheduleStatus(schedule: Schedule, today: string): ScheduleStatus {
  if (!schedule.active) return 'paused';
  if (!schedule.nextDueDate) return 'completed';
  return schedule.nextDueDate <= today ? 'overdue' : 'upcoming';
}

/** A reminder can be posted once its due date has arrived (never before). */
export function isPostable(nextDueDate: string, today: string): boolean {
  return nextDueDate <= today;
}

/** "3 of 12" / "3 posted" / "—" occurrence progress for table rows. */
export function occurrenceLabel(schedule: Schedule): string {
  if (schedule.maxOccurrences) return `${schedule.occurrencesPosted} of ${schedule.maxOccurrences}`;
  if (schedule.occurrencesPosted > 0) return `${schedule.occurrencesPosted} posted`;
  return '—';
}
