// Applies the persisted theme + locale + accent before SvelteKit hydrates to
// avoid a light-mode flash. Loaded as an external file because the app CSP
// only allows 'self' scripts. Keep in sync with $lib/stores/settings.svelte.ts.
(function () {
  try {
    var stored = JSON.parse(localStorage.getItem('spendo-settings') || '{}');
    var theme = stored.theme === 'dark' || stored.theme === 'system' ? stored.theme : 'light';
    var dark =
      theme === 'dark' ||
      (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
    var root = document.documentElement;
    root.classList.toggle('dark', dark);
    // Compact is the only density toggle; keep in sync with applyDensity().
    root.classList.toggle('compact', stored.density === 'compact');
    root.style.colorScheme = dark ? 'dark' : 'light';
    if (typeof stored.locale === 'string' && stored.locale) root.lang = stored.locale;
    var accents = ['zinc', 'blue', 'emerald', 'amber', 'rose', 'violet', 'custom'];
    var accent = accents.indexOf(stored.accent) >= 0 ? stored.accent : 'zinc';
    root.dataset.accent = accent;
    var custom = typeof stored.customPrimary === 'string' ? stored.customPrimary : null;
    if (accent === 'custom' && custom && /^#[0-9a-fA-F]{6}$/.test(custom)) {
      var channel = function (i) {
        var c = parseInt(custom.slice(i, i + 2), 16) / 255;
        return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
      };
      var luminance = 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
      var foreground = luminance > 0.18 ? '#18181b' : '#fafafa';
      root.style.setProperty('--primary', custom);
      root.style.setProperty('--primary-foreground', foreground);
      root.style.setProperty('--ring', custom);
      root.style.setProperty('--chart-1', custom);
      root.style.setProperty('--sidebar-primary', custom);
      root.style.setProperty('--sidebar-primary-foreground', foreground);
      root.style.setProperty('--sidebar-ring', custom);
    }
  } catch {
    /* malformed settings: fall back to light */
  }
})();
