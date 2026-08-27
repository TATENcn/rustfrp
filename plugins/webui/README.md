# RustFRP WebUI

The daemon-embedded WebUI uses Vue 3, TypeScript, Vite, Bun, Pinia, Vue Router,
and Naive UI. User-facing text uses the typed message maps under `src/i18n`,
while locale-sensitive data formatting is centralized in `src/i18n/format.ts`
using the standard ECMA-402 `Intl` APIs.

`Intl.DurationFormat` has a built-in fallback for older browsers. Byte values
remain IEC/base-1024 (KiB, MiB, GiB); `Intl` formats only their numeric portion.

## Recommended IDE Setup

[VS Code](https://code.visualstudio.com/) + [Vue (Official)](https://marketplace.visualstudio.com/items?itemName=Vue.volar) (and disable Vetur).

## Recommended Browser Setup

- Chromium-based browsers (Chrome, Edge, Brave, etc.):
  - [Vue.js devtools](https://chromewebstore.google.com/detail/vuejs-devtools/nhdogjmejiglipccpnnnanhbledajbpd)
  - [Turn on Custom Object Formatter in Chrome DevTools](http://bit.ly/object-formatters)
- Firefox:
  - [Vue.js devtools](https://addons.mozilla.org/en-US/firefox/addon/vue-js-devtools/)
  - [Turn on Custom Object Formatter in Firefox DevTools](https://fxdx.dev/firefox-devtools-custom-object-formatters/)

## Type Support for `.vue` Imports in TS

TypeScript cannot handle type information for `.vue` imports by default, so we replace the `tsc` CLI with `vue-tsc` for type checking. In editors, we need [Volar](https://marketplace.visualstudio.com/items?itemName=Vue.volar) to make the TypeScript language service aware of `.vue` types.

## Customize configuration

See [Vite Configuration Reference](https://vite.dev/config/).

## Project Setup

```sh
bun install
```

### Compile and Hot-Reload for Development

```sh
bun dev
```

### Type-Check, Compile and Minify for Production

```sh
bun run build
```

### Unit Tests

```sh
bun test
```

Both the main CI workflow and WebUI CI run the unit tests, translation-key
check, TypeScript check, and production build.
