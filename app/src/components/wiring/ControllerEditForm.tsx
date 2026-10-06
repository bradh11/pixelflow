import { useState } from "react";
import type { Controller } from "../../api/types";
import { type ControllerDraft, MAX_UNIVERSE, controllerDraft, controllerEdits, draftProblems } from "../../lib/controllerEdit";
import { useApp } from "../../state/store";
import { Button, Input, Select } from "../ui";

function Problem({ id, text }: { id: string; text: string | undefined }) {
  if (!text) return null;
  return (
    <p id={id} className="text-xs text-red-600 dark:text-red-400">
      {text}
    </p>
  );
}

/**
 * Change a controller's name, address, and protocol where it is. Saving is one undo step and keeps
 * the controller's wiring, its kind, and what was found on the network.
 */
export function ControllerEditForm({ controller, onDone }: { controller: Controller; onDone: () => void }) {
  const show = useApp((s) => s.snapshot?.show);
  const apply = useApp((s) => s.apply);
  const [draft, setDraft] = useState<ControllerDraft>(() => controllerDraft(controller));
  const [touched, setTouched] = useState(false);
  if (!show) return null;
  const problems = draftProblems(draft, show, controller.id);
  const ok = Object.keys(problems).length === 0;
  const set = (change: Partial<ControllerDraft>) => {
    setTouched(true);
    setDraft({ ...draft, ...change });
  };
  const fieldId = (key: string) => `controller-${controller.id}-${key}`;
  // A problem shows once the user has changed something (an empty name at first isn't shouted at).
  const shown = touched ? problems : {};
  const save = async () => {
    if (!ok) return;
    if (await apply(controllerEdits(controller.id, draft))) onDone();
  };
  return (
    <form
      aria-label={`Edit ${controller.name}`}
      className="grid grid-cols-1 gap-3 border-t border-neutral-200 bg-neutral-50 p-3 sm:grid-cols-2 lg:grid-cols-4 dark:border-neutral-800 dark:bg-neutral-950/60"
      onSubmit={(e) => {
        e.preventDefault();
        void save();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          onDone();
        }
      }}
    >
      <label className="flex flex-col gap-1 text-sm">
        <span className="text-neutral-600 dark:text-neutral-400">Name</span>
        <Input
          autoFocus
          value={draft.name}
          aria-invalid={!!shown.name}
          aria-describedby={shown.name ? fieldId("name") : undefined}
          onChange={(e) => set({ name: e.target.value })}
        />
        <Problem id={fieldId("name")} text={shown.name} />
      </label>
      <label className="flex flex-col gap-1 text-sm">
        <span className="text-neutral-600 dark:text-neutral-400">IP address</span>
        <Input
          value={draft.address}
          placeholder="e.g. 192.168.1.50"
          aria-invalid={!!shown.address}
          aria-describedby={shown.address ? fieldId("address") : undefined}
          onChange={(e) => set({ address: e.target.value })}
        />
        <Problem id={fieldId("address")} text={shown.address} />
      </label>
      <label className="flex flex-col gap-1 text-sm">
        <span className="text-neutral-600 dark:text-neutral-400">Protocol</span>
        <Select value={draft.protocol} onChange={(e) => set({ protocol: e.target.value as ControllerDraft["protocol"] })}>
          <option value="ddp">DDP</option>
          <option value="sacn">sACN (E1.31)</option>
        </Select>
      </label>
      {draft.protocol === "sacn" ? (
        <>
          <label className="flex flex-col gap-1 text-sm" title="The first universe this controller listens on. Leave it empty and PixelFlow picks one that doesn't clash.">
            <span className="text-neutral-600 dark:text-neutral-400">Start universe</span>
            <Input
              inputMode="numeric"
              value={draft.startUniverse}
              placeholder="Automatic"
              aria-invalid={!!shown.startUniverse}
              aria-describedby={shown.startUniverse ? fieldId("universe") : undefined}
              onChange={(e) => set({ startUniverse: e.target.value })}
            />
            <Problem id={fieldId("universe")} text={shown.startUniverse} />
          </label>
          <label className="flex flex-col gap-1 text-sm" title="510 fits exactly 170 RGB pixels in each universe. Match what the controller is set to.">
            <span className="text-neutral-600 dark:text-neutral-400">Channels per universe</span>
            <Select value={draft.universeSize} onChange={(e) => set({ universeSize: Number(e.target.value) === 512 ? 512 : 510 })}>
              <option value={510}>510</option>
              <option value={512}>512</option>
            </Select>
          </label>
          <label className="flex items-center gap-2 self-end pb-2 text-sm">
            <input type="checkbox" checked={draft.multicast} onChange={(e) => set({ multicast: e.target.checked })} className="accent-accent-500" />
            Multicast
          </label>
        </>
      ) : (
        <p className="self-end pb-2 text-xs text-neutral-500">DDP needs no universes: channels follow the wiring.</p>
      )}
      <div className="col-span-full flex flex-wrap items-center gap-2">
        <Button type="submit" variant="primary" disabled={!ok}>
          Save
        </Button>
        <Button variant="ghost" onClick={onDone}>
          Cancel
        </Button>
        <span className="text-xs text-neutral-500">
          The wiring stays as it is.
          {controller.sequenceChannels && " So does where it sits in FPP sequences."}
          {draft.protocol === "sacn" && ` Universes go from 1 to ${MAX_UNIVERSE.toLocaleString("en-US")}.`}
        </span>
      </div>
    </form>
  );
}
