// Tauri serves a static bundle (adapter-static + index.html fallback) with the
// Rust backend over IPC — there is no SSR server at runtime. Disabling SSR
// makes full reloads boot client-side instead of painting server-rendered
// HTML baked with a null session (chromeless MinimalTitleBar + page
// skeletons, no sidebar/titlebar) before the client could run.
export const ssr = false;
