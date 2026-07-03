import { query } from '$app/server';
import { getAccounts, type AccountsResult } from '$lib/server/accounts';

export const fetchAccounts = query(
  'unchecked',
  async ({ token }: { token: string }): Promise<AccountsResult> => {
    return getAccounts(token);
  }
);
