import type { Attachment } from 'svelte/attachments';

interface TooltipOptions {
  /** Only show when the element’s text is cut off (truncated cells and labels). */
  truncated?: boolean;
}

const SHOW_DELAY_MS = 400;
const GAP_PX = 6;
const EDGE_PX = 8;

let seq = 0;

/**
 * Themed hover/focus tooltip, replacing native `title` hints (slow, unstyled,
 * ignore dark mode). Usage: `<Button {@attach tooltip('Delete')}>`. A null or
 * empty text renders nothing, so conditional hints need no wrapper.
 */
export function tooltip(
  text: string | null | undefined,
  { truncated = false }: TooltipOptions = {}
): Attachment<HTMLElement> {
  return (node) => {
    if (!text) return;
    const label = text;
    const id = `tooltip-${++seq}`;
    let tip: HTMLDivElement | null = null;
    let timer: ReturnType<typeof setTimeout> | undefined;

    function place(el: HTMLDivElement) {
      const anchor = node.getBoundingClientRect();
      const box = el.getBoundingClientRect();
      let top = anchor.top - box.height - GAP_PX;
      if (top < EDGE_PX) top = anchor.bottom + GAP_PX;
      const centered = anchor.left + anchor.width / 2 - box.width / 2;
      const left = Math.min(Math.max(EDGE_PX, centered), window.innerWidth - box.width - EDGE_PX);
      el.style.top = `${top}px`;
      el.style.left = `${left}px`;
    }

    function open() {
      if (tip || (truncated && node.scrollWidth <= node.clientWidth)) return;
      tip = document.createElement('div');
      tip.id = id;
      tip.role = 'tooltip';
      tip.textContent = label;
      tip.className =
        'pointer-events-none fixed z-50 max-w-xs rounded-md bg-foreground px-3 py-1.5 text-xs text-background shadow-sm animate-in fade-in-0 zoom-in-95';
      document.body.append(tip);
      place(tip);
      node.setAttribute('aria-describedby', id);
    }

    function show() {
      clearTimeout(timer);
      timer = setTimeout(open, SHOW_DELAY_MS);
    }

    function hide() {
      clearTimeout(timer);
      tip?.remove();
      tip = null;
      if (node.getAttribute('aria-describedby') === id) node.removeAttribute('aria-describedby');
    }

    function onFocusIn() {
      if (node.matches(':focus-visible')) show();
    }

    function onKeydown(e: KeyboardEvent) {
      if (e.key === 'Escape') hide();
    }

    node.addEventListener('pointerenter', show);
    node.addEventListener('pointerleave', hide);
    node.addEventListener('pointerdown', hide);
    node.addEventListener('focusin', onFocusIn);
    node.addEventListener('focusout', hide);
    node.addEventListener('keydown', onKeydown);
    window.addEventListener('scroll', hide, true);

    return () => {
      hide();
      node.removeEventListener('pointerenter', show);
      node.removeEventListener('pointerleave', hide);
      node.removeEventListener('pointerdown', hide);
      node.removeEventListener('focusin', onFocusIn);
      node.removeEventListener('focusout', hide);
      node.removeEventListener('keydown', onKeydown);
      window.removeEventListener('scroll', hide, true);
    };
  };
}
