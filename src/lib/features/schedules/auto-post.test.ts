import { describe, expect, it, vi } from 'vitest';

vi.mock('$lib/components/ui/sonner', () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
vi.mock('$lib/stores/db.svelte', () => ({ refreshSession: vi.fn() }));

const { autoPostMessages } = await import('./auto-post');

describe('autoPostMessages', () => {
  it('stays quiet when nothing happened', () => {
    expect(autoPostMessages({ posted: 0, failed: [] })).toEqual({});
  });

  it('counts posts and names failed schedules', () => {
    const messages = autoPostMessages({
      posted: 3,
      failed: [
        { guid: 'a', name: 'Laptop', message: 'x' },
        { guid: 'b', name: 'Phone', message: 'y' }
      ]
    });
    expect(messages.posted).toBe('Posted 3 scheduled transactions');
    expect(messages.failed).toBe('Couldn’t auto-post Laptop, Phone. Open Scheduled to fix it');
  });

  it('uses the singular for one post', () => {
    expect(autoPostMessages({ posted: 1, failed: [] }).posted).toBe(
      'Posted 1 scheduled transaction'
    );
  });
});
