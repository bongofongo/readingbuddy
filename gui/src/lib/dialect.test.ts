/**
 * The eslint config is a guard, so it is tested like one.
 *
 * `gui/CLAUDE.md`: *"An absent rule reads exactly like a passing one."* That is
 * the whole reason this file exists. Svelte 4 syntax still **compiles** under
 * Svelte 5, and the training mass for Svelte is overwhelmingly Svelte 4 — so the
 * ban is the single load-bearing lint rule in this project, and a typo in a
 * selector would silently disarm it while `make web-check` stayed green.
 *
 * This asserts the selectors match what they claim to and reject nothing else.
 * It does not run eslint (that would be a second copy of the config's
 * resolution); it asserts the selector strings themselves against the AST
 * eslint's own parser produces.
 */
import { Linter } from 'eslint';
import { describe, expect, it } from 'vitest';

import { svelte4Bans } from '../../eslint.config.js';

const linter = new Linter();

function lint(code: string) {
  return linter.verify(code, {
    languageOptions: { ecmaVersion: 2022, sourceType: 'module' },
    rules: { 'no-restricted-syntax': ['error', ...svelte4Bans] },
  });
}

describe('the Svelte 4 ban', () => {
  const rejected: [string, string][] = [
    ['props', 'export let title = "x";'],
    ['reactive statements', 'let a = 1; let b; $: b = a * 2;'],
    ['reactive blocks', 'let a = 1; $: { console.log(a); }'],
    [
      'event dispatchers',
      'import { createEventDispatcher } from "svelte"; const d = createEventDispatcher();',
    ],
    ['writable stores', 'import { writable } from "svelte/store"; const x = writable(0);'],
  ];

  for (const [what, code] of rejected) {
    it(`rejects Svelte 4 ${what}`, () => {
      const found = lint(code);
      expect(found.length, `no rule fired on: ${code}`).toBeGreaterThan(0);
    });
  }

  const accepted: [string, string][] = [
    ['runes props', 'const { title } = $props();'],
    ['derived', 'const a = 1; const b = $derived(a * 2);'],
    ['state', 'let n = $state(0);'],
    ['callback props', 'const { onpick } = $props(); onpick(3);'],
    // A `const` export is how this module exports `svelte4Bans` itself, and
    // banning it would make the config unlintable. The selector is specifically
    // `kind='let'`, and this is what pins that.
    ['exported consts', 'export const bans = [];'],
    // A store imported for something other than component state is legitimate —
    // `readable` and `derived` from `svelte/store` are not the banned pattern.
    ['other store imports', 'import { readable } from "svelte/store"; readable(0);'],
  ];

  for (const [what, code] of accepted) {
    it(`accepts ${what}`, () => {
      expect(lint(code), `a rule fired on legitimate code: ${code}`).toEqual([]);
    });
  }
});
