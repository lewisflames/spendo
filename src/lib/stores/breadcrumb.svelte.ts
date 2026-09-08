import { page } from '$app/state';

export interface BreadcrumbItem {
  name: string;
  href?: string;
}

export const breadcrumb = $state<{ value: { items: BreadcrumbItem[]; route: string | null } }>({
  value: { items: [], route: null }
});

export function setBreadcrumb(items: BreadcrumbItem[]) {
  // Replace the whole object (single notification), mirroring the other
  // stores — never mutate `items`/`route` in place.
  breadcrumb.value = { items, route: page.url.pathname };
}
