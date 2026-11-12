// The theme's CSS custom properties, which the shell makes from the theme
// setting (AGENTS.md section 7.4). Following the system, the light or dark
// set applies by the system's mode.
import type { Look } from '../ipc/types';
import './base.css';

const dark = window.matchMedia('(prefers-color-scheme: dark)');
let current: Look | null = null;

function paint(): void {
  if (!current) return;
  const { theme } = current;
  const vars = theme.followSystem && !dark.matches ? theme.light : theme.followSystem ? theme.dark : theme.light;
  const root = document.documentElement;
  for (const v of vars) root.style.setProperty(v.name, v.value);
  const scheme = vars.find((v) => v.name === '--scheme')?.value ?? 'light';
  root.style.colorScheme = scheme;
  root.dataset.scheme = scheme;
  root.lang = current.language;
  root.dir = current.rightToLeft ? 'rtl' : 'ltr';
}

dark.addEventListener('change', paint);

/** Apply the look the shell sent. */
export function applyLook(look: Look | null): void {
  current = look;
  paint();
}
