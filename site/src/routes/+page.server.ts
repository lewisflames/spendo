import { env } from '$env/dynamic/private';
import { fetchLatestRelease } from '$lib/release';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ fetch }) => ({
  release: await fetchLatestRelease(fetch, env.GITHUB_TOKEN)
});
