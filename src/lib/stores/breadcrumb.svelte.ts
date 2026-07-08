import { page } from '$app/state';

export interface BreadcrumbItem {
  name: string;
  href?: string;
}

export const breadcrumb = $state<{ value: { items: BreadcrumbItem[]; route: string | null } }>({
  value: { items: [], route: null }
});

export function setBreadcrumb(items: BreadcrumbItem[]) {
  breadcrumb.value.items = items;
  breadcrumb.value.route = page.url.pathname;
}
