import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** Svelte 5 with TypeScript (AGENTS.md section 7.4). */
export default {
  preprocess: vitePreprocess(),
  compilerOptions: { runes: true },
};
