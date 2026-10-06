import { useApp } from "./store";
import { toast } from "./toast";

/**
 * A toast for a change that's easy to regret ("Deleted Arch 1"), with Undo. Undo only takes the
 * change back while it's still the latest one (the show is at `revision`); after something else
 * has changed, the toast says to use Undo instead of undoing the wrong thing.
 */
export function toastWithUndo(text: string, revision: number | null) {
  if (revision === null) return;
  toast(text, {
    label: "Undo",
    run: () => {
      const app = useApp.getState();
      if (app.snapshot?.revision === revision) return app.undo();
      toast("Other changes came after that one: use Undo (⌘Z) to step back through them.");
    },
  });
}
