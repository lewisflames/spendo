<!-- Fixed at z-0 (not negative: negative z-index paints behind the page
  canvas background in some WebViews) so the grid extends under the
  drag region and window controls instead of leaving a solid bar on top.
  Content and the minimal title bar sit at z-10 above it. -->
<div aria-hidden="true" class="pointer-events-none fixed inset-0 z-0 overflow-hidden">
  <!-- Faint ledger grid, faded toward the edges (static by design: cheap) -->
  <div
    class="absolute inset-0 [background-image:linear-gradient(to_right,var(--border)_1px,transparent_1px),linear-gradient(to_bottom,var(--border)_1px,transparent_1px)] [mask-image:radial-gradient(ellipse_75%_65%_at_50%_40%,black,transparent)] [background-size:44px_44px] opacity-60"
  ></div>
</div>
