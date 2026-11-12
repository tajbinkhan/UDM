import js from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';
import svelteConfig from './svelte.config.js';

export default ts.config(
  { ignores: ['dist/', 'node_modules/', 'src/bindings.ts'] },
  js.configs.recommended,
  ...ts.configs.recommended,
  ...svelte.configs.recommended,
  {
    languageOptions: { globals: { ...globals.browser, ...globals.node } },
    rules: {
      // AGENTS.md section 11: no console.log in committed code.
      'no-console': 'error',
      // AGENTS.md section 7.4: text from the internet is never HTML.
      'svelte/no-at-html-tags': 'error',
    },
  },
  {
    files: ['**/*.svelte', '**/*.svelte.ts'],
    languageOptions: { parserOptions: { projectService: true, extraFileExtensions: ['.svelte'], parser: ts.parser, svelteConfig } },
  },
  {
    // AGENTS.md section 7.4: only src/ipc/ reaches the shell.
    files: ['src/**/*.ts', 'src/**/*.svelte'],
    ignores: ['src/ipc/**'],
    rules: {
      'no-restricted-imports': ['error', { patterns: ['@tauri-apps/*', '**/bindings', '**/bindings.ts'] }],
    },
  },
);
