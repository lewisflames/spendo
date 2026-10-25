const REPO = 'lewisflames/spendo';

export const REPO_URL = `https://github.com/${REPO}`;
export const RELEASES_URL = `${REPO_URL}/releases/latest`;

export type Release = {
  version: string | null;
  publishedAt: string | null;
  mac: string;
  appImage: string;
  deb: string;
};

type GithubRelease = {
  tag_name: string;
  published_at: string;
  assets: { name: string; browser_download_url: string }[];
};

/** Release asset names carry the version, so resolve the direct links at build time. */
export async function fetchLatestRelease(fetch: typeof globalThis.fetch, token?: string) {
  const fallback: Release = {
    version: null,
    publishedAt: null,
    mac: RELEASES_URL,
    appImage: RELEASES_URL,
    deb: RELEASES_URL
  };
  const headers: Record<string, string> = { accept: 'application/vnd.github+json' };
  if (token) headers.authorization = `Bearer ${token}`;

  try {
    const res = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers });
    if (!res.ok) throw new Error(`GitHub API ${res.status}`);
    const release: GithubRelease = await res.json();
    const asset = (suffix: string) =>
      release.assets.find((a) => a.name.endsWith(suffix))?.browser_download_url ?? RELEASES_URL;
    return {
      version: release.tag_name.replace(/^v/, ''),
      publishedAt: release.published_at,
      mac: asset('_universal.dmg'),
      appImage: asset('_amd64.AppImage'),
      deb: asset('_amd64.deb')
    } satisfies Release;
  } catch (e) {
    // Offline local builds still work; the buttons just open the releases page.
    console.warn(`Could not resolve the latest release, linking to ${RELEASES_URL}:`, e);
    return fallback;
  }
}
