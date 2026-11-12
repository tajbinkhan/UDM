// The grabber's window state: the wizard's fields and the results' ticks
// (AGENTS.md section 7.4: drafts live in the window).
import type { GrabDraft, GrabScheduleDraft } from '../../ipc/types';

class Grab {
  draft = $state<GrabDraft | null>(null);
  /** The project's schedule, as Options > Schedule left it. */
  schedule = $state<GrabScheduleDraft | null>(null);
  checked = $state<boolean[]>([]);
  /** The results the ticks belong to. */
  results = -1;
}

export const grab = new Grab();
