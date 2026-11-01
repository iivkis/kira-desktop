# Kira Desktop contributor and agent rules

## Visual stack (mandatory)

- **Apache ECharts** is the project's charting library for all new or redesigned
  charts, gauges, trends and quantitative visualizations. Do not hand-draw charts
  with ad-hoc CSS bars, SVG paths or an additional charting library.
  Prefer tree-shakable `echarts/core` imports, register only required modules
  and choose SVG rendering for compact WebKitGTK overlays.
- **GSAP** is the project's JavaScript animation library. Use it for deliberate
  motion such as entry transitions, numeric emphasis and state changes rather
  than introducing a second JS animation framework or complex CSS keyframes.
- Respect `prefers-reduced-motion` everywhere, and retain accessible labels,
  meaningful numeric fallback text, keyboard focus and functional no-motion UI.
  Never animate fake data into a chart or replace missing sensor data with zero.
- **Lifecycle matters:** WebKit widgets mount only while requested. Instantiate
  ECharts and GSAP effects on mount, resize charts when geometry changes,
  dispose chart instances, kill GSAP tweens and detach observers on unmount.
  Never create unmanaged timers, background data polling or repeated listeners.
- **Offline WebKit constraint:** The packaged frontend is a single
  self-contained HTML document. All npm code is bundled locally, with no CDN,
  runtime fetches, unexpected JS chunks or external stylesheets. Keep
  `frontend/scripts/inline-assets.mjs` and its tests passing.
- Retain the canonical five-color Kira palette in
  `frontend/src/lib/themes/kira-default.css`, the 42px top bar, and the
  locked-down Rust/GIO ↔ WebKit bridge. Charts are read-only presentation.

## Build and review

Changes to runtime JS dependencies require updates to both
`frontend/package.json` and `frontend/package-lock.json`, plus the
frontend Nix dependency hash in `nix/package.nix`. Run `task check`,
`task frontend:build` and (for frontend lock updates) `nix build .#frontend`.
Test supported and unavailable metrics, bounded history, reduced motion,
and open/close cleanup. Visual-QA check the popup in isolated nested Niri.
