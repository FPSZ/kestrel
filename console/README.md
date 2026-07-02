# Kestrel Console

WebUI for Kestrel - a flat, dark, single-accent console in the Linear / Raycast
lineage. Peer to the CLI: a pure event renderer + Op sender over the same core
(see [ADR-0007](../docs/adr/0007-webui-browser-axum.md)).

## Dev

```sh
npm install
npm run dev        # Vite on :7823, proxies /api -> kestrel-server on :4321
```

The shell runs standalone (no backend needed) to preview the design. Live chat,
permission modals, and session replay light up once `kestrel-server` (SSE + ops)
is wired.

## Design

Dark, flat, one accent hue. Depth comes from hairline borders, not shadows -
no glossy glass highlights. The topbar and sidebar are fused into one frosted
surface; the content pane is an inset bezel. All design tokens live in
`src/index.css` (`@theme`) - restyle by editing tokens only.
