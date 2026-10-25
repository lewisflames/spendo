/** Mirrored from `tags.rs` (`MAX_TAG_LEN`, `MAX_TAGS_PER_TX`). */
export const MAX_TAG_LEN = 40;
export const MAX_TAGS_PER_TX = 20;

/** Trim and collapse inner whitespace, like the backend does before saving. */
export function normalizeTagName(raw: string): string {
  return raw.trim().split(/\s+/).filter(Boolean).join(' ');
}

/** Why `name` (already normalized) can't be a tag, or `null` when it can. */
export function tagNameError(name: string): string | null {
  if (!name) return 'Tags need a name';
  if ([...name].length > MAX_TAG_LEN) return `Keep tags under ${MAX_TAG_LEN} characters`;
  if (name.includes(',')) return 'Tags can’t contain commas';
  return null;
}

/**
 * `tags` plus `raw`, reusing the spelling of a known tag that matches ignoring
 * case. Adding a tag the list already has is a no-op, not an error.
 */
export function addTagName(
  tags: readonly string[],
  raw: string,
  known: readonly string[] = []
): { tags: string[]; error: string | null } {
  const name = normalizeTagName(raw);
  const error = tagNameError(name);
  if (error) return { tags: [...tags], error };
  const lower = name.toLowerCase();
  if (tags.some((t) => t.toLowerCase() === lower)) return { tags: [...tags], error: null };
  if (tags.length >= MAX_TAGS_PER_TX) {
    return { tags: [...tags], error: `Keep it to ${MAX_TAGS_PER_TX} tags or fewer` };
  }
  const spelled = known.find((k) => k.toLowerCase() === lower) ?? name;
  return { tags: [...tags, spelled], error: null };
}

/** Known tags not already picked, matching `query` anywhere (prefix matches first). */
export function tagSuggestions(
  known: readonly string[],
  picked: readonly string[],
  query: string,
  limit = 8
): string[] {
  const taken = new Set(picked.map((t) => t.toLowerCase()));
  const q = normalizeTagName(query).toLowerCase();
  const open = known.filter((k) => !taken.has(k.toLowerCase()));
  if (!q) return open.slice(0, limit);
  const starts = open.filter((k) => k.toLowerCase().startsWith(q));
  const contains = open.filter(
    (k) => !k.toLowerCase().startsWith(q) && k.toLowerCase().includes(q)
  );
  return [...starts, ...contains].slice(0, limit);
}

/** Same tags ignoring order. */
export function sameTags(a: readonly string[], b: readonly string[]): boolean {
  if (a.length !== b.length) return false;
  const set = new Set(a);
  return b.every((t) => set.has(t));
}
