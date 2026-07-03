import { query } from '$app/server';
import { getDashboardData, type DashboardData } from '$lib/server/dashboard-data';

export const fetchDashboardData = query(
  'unchecked',
  async ({
    token,
    monthCount
  }: {
    token: string;
    monthCount?: number | null;
  }): Promise<DashboardData> => {
    return getDashboardData(token, monthCount);
  }
);
