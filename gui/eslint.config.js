import js from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';

/**
 * The Svelte 4 ban, and why it is a lint rule rather than a note in CLAUDE.md.
 *
 * Svelte 4 syntax still COMPILES under Svelte 5. So an agent writing
 * `export let foo` gets a working component, a passing build, and a codebase in
 * two dialects — and the training mass for Svelte is overwhelmingly Svelte 4,
 * which makes this the single most likely defect in generated code here.
 *
 * `src/lib/dialect.test.ts` asserts these rules actually fire. An absent rule
 * reads exactly like a passing one, and this file is the one place in the repo
 * where that failure is invisible.
 */
export const svelte4Bans = [
  {
    selector: "ExportNamedDeclaration > VariableDeclaration[kind='let']",
    message: 'Svelte 4 props. Use `let { foo } = $props()`.',
  },
  {
    selector: "LabeledStatement[label.name='$']",
    message: 'Svelte 4 reactive statement. Use `$derived` or `$effect`.',
  },
  {
    selector: "CallExpression[callee.name='createEventDispatcher']",
    message: 'Svelte 4 events. Use callback props.',
  },
  {
    selector: "ImportDeclaration[source.value='svelte/store'] ImportSpecifier[imported.name='writable']",
    message: 'A writable store for component state is Svelte 4. Use `$state()`.',
  },
];

export default ts.config(
  js.configs.recommended,
  ...ts.configs.recommended,
  ...svelte.configs['flat/recommended'],
  {
    languageOptions: { globals: { ...globals.browser, ...globals.node } },
    rules: {
      'no-restricted-syntax': ['error', ...svelte4Bans],
      // A leading underscore is the conventional "deliberately unused", and a
      // parameter a fake must accept and must NOT act on is exactly that case.
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_' },
      ],
    },
  },
  {
    files: ['**/*.svelte'],
    languageOptions: { parserOptions: { parser: ts.parser } },
    rules: {
      'svelte/valid-compile': 'error',
      'svelte/no-export-load-in-svelte-module-in-kit-pages': 'error',
      'no-restricted-syntax': ['error', ...svelte4Bans],
    },
  },
  {
    // Generated. `make ts` owns every byte; a lint fix here is reverted by the
    // next regeneration and `make ts-check` then fails for a reason nobody wrote.
    ignores: [
      'src/lib/api/bindings.ts',
      'build/',
      '.svelte-kit/',
      'src-tauri/',
      'tests/shots/',
    ],
  },
);
