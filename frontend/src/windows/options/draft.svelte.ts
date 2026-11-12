// The Options window's draft: a copy of the settings the pages edit while
// the window is open (AGENTS.md section 7.4: a field's draft is window
// state). OK sends it whole; the shell checks it.
import type { OptionsDraft } from '../../ipc/types';

class Editing {
  draft = $state<OptionsDraft | null>(null);
}

export const editing = new Editing();
