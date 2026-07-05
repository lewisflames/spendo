import { z } from 'zod';
import { query } from '$app/server';
import { getAccounts, type AccountsResult } from '$lib/server/accounts';

export const fetchAccounts = query(
  z.object({}).strict(),
  async (_data: Record<string, never>): Promise<AccountsResult> => {
    return getAccounts();
  }
);
