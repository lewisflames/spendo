// Applies the persisted theme + locale before SvelteKit hydrates to avoid a
// light-mode flash. Loaded as an external file because the app CSP only
// allows 'self' scripts. Keep in sync with $lib/stores/settings.svelte.ts.
(function () {
  try {
    var stored = JSON.parse(localStorage.getItem('spendo-settings') || '{}');
    var theme = stored.theme === 'dark' || stored.theme === 'system' ? stored.theme : 'light';
    var dark =
      theme === 'dark' ||
      (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
    var root = document.documentElement;
    root.classList.toggle('dark', dark);
    root.style.colorScheme = dark ? 'dark' : 'light';
    if (typeof stored.locale === 'string' && stored.locale) root.lang = stored.locale;
  } catch {
    /* malformed settings: fall back to light */
  }
})();
