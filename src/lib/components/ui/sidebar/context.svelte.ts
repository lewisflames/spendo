import { IsMobile } from '$lib/hooks/is-mobile.svelte.js';
import { appSettings, saveSettings } from '$lib/stores/settings.svelte';
import { getContext, setContext } from 'svelte';

class SidebarState {
  #isMobile: IsMobile;

  constructor() {
    this.#isMobile = new IsMobile();
  }

  get isMobile() {
    return this.#isMobile.current;
  }

  get collapsed() {
    return appSettings.value.sidebarCollapsed;
  }

  toggle() {
    saveSettings({ sidebarCollapsed: !appSettings.value.sidebarCollapsed });
  }
}

const SYMBOL_KEY = 'scn-sidebar';

export function setSidebar(): SidebarState {
  return setContext(Symbol.for(SYMBOL_KEY), new SidebarState());
}

export function useSidebar(): SidebarState {
  return getContext(Symbol.for(SYMBOL_KEY));
}
