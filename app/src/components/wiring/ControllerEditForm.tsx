import { useEffect, useId, useState, type ReactNode } from "react";
import type { Controller } from "../../api/types";
import { type ControllerDraft, type DraftProblems, MAX_UNIVERSE, MAX_UNIVERSE_SIZE, checkDraft, controllerDraft, controllerEdits } from "../../lib/controllerEdit";
import { useApp } from "../../state/store";
import { Button, Input, More, Select } from "../ui";

const LEGEND = "col-span-full mb-1 text-xs font-semibold tracking-wide text-neutral-500 uppercase";

const FIELD_NAMES: Record<keyof DraftProblems, string> = {
  name: "name",
  address: "address",
  startUniverse: "start universe",
  universeSize: "channels per universe",
};

/** A field with its label, and under it (outside the label, so it's read once) a problem or a warning. */
function Field({
  label,
  id,
  problem,
  warning,
  hint,
  children,
}: {
  label: string;
  id: string;
  problem?: string;
  warning?: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1 text-sm" title={hint}>
      <label htmlFor={id} className="text-neutral-600 dark:text-neutral-400">
        {label}
      </label>
      {children}
      {problem ? (
        <p id={`${id}-note`} className="text-xs text-red-600 dark:text-red-400">
          {problem}
        </p>
      ) : warning ? (
        <p id={`${id}-note`} className="text-xs text-amber-700 dark:text-amber-400">
          {warning}
        </p>
      ) : null}
    </div>
  );
}

/**
 * Change a controller's name, address, and protocol where it is. Saving is one undo step and keeps
 * the controller's wiring, its kind, and what was found on the network. Only what the user changes
 * is checked, so imported controllers (split ones sharing an address, multicast ones with none)
 * can be edited too.
 */
export function ControllerEditForm({ controller, onDone }: { controller: Controller; onDone: () => void }) {
  const show = useApp((s) => s.snapshot?.show);
  const apply = useApp((s) => s.apply);
  const base = useId();
  const [draft, setDraft] = useState<ControllerDraft>(() => controllerDraft(controller));
  const [touched, setTouched] = useState(false);
  // Until the user types, the form follows the controller (an undo, say, changing it underneath).
  useEffect(() => {
    if (!touched) setDraft(controllerDraft(controller));
  }, [controller, touched]);
  if (!show) return null;
  const { problems, warnings } = checkDraft(draft, controller, show);
  const blocking = (Object.keys(problems) as (keyof DraftProblems)[]).map((k) => FIELD_NAMES[k]);
  const ok = blocking.length === 0;
  const set = (change: Partial<ControllerDraft>) => {
    setTouched(true);
    setDraft({ ...draft, ...change });
  };
  const id = (key: string) => `${base}-${key}`;
  const described = (key: keyof DraftProblems) => (problems[key] || warnings[key] ? `${id(key)}-note` : undefined);
  const save = async () => {
    if (!ok) return;
    if (await apply(controllerEdits(controller.id, draft))) onDone();
  };
  const multicast = draft.protocol === "sacn" && draft.multicast;
  return (
    <form
      aria-label={`Edit ${controller.name}`}
      className="flex flex-col gap-3 border-t border-neutral-200 bg-neutral-50 p-3 dark:border-neutral-800 dark:bg-neutral-950/60"
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
      <div className="grid grid-cols-1 gap-x-6 gap-y-3 lg:grid-cols-2">
        <fieldset className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <legend className={LEGEND}>Controller</legend>
          <Field label="Name" id={id("name")} problem={problems.name} warning={warnings.name}>
            <Input
              id={id("name")}
              autoFocus
              value={draft.name}
              aria-invalid={!!problems.name}
              aria-describedby={described("name")}
              onChange={(e) => set({ name: e.target.value })}
            />
          </Field>
          <Field label="IP address" id={id("address")} problem={problems.address} warning={warnings.address}>
            <Input
              id={id("address")}
              value={draft.address}
              placeholder={multicast ? "Not needed for multicast" : "e.g. 192.168.1.50"}
              aria-invalid={!!problems.address}
              aria-describedby={described("address")}
              onChange={(e) => set({ address: e.target.value })}
            />
          </Field>
        </fieldset>
        <fieldset className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <legend className={LEGEND}>Output</legend>
          <Field label="Protocol" id={id("protocol")}>
            <Select id={id("protocol")} value={draft.protocol} onChange={(e) => set({ protocol: e.target.value as ControllerDraft["protocol"] })}>
              <option value="ddp">DDP</option>
              <option value="sacn">sACN (E1.31)</option>
            </Select>
          </Field>
          {draft.protocol === "sacn" ? (
            <Field
              label="Start universe"
              id={id("startUniverse")}
              problem={problems.startUniverse}
              hint="The first universe this controller listens on. Leave it empty and PixelFlow picks one that doesn't clash."
            >
              <Input
                id={id("startUniverse")}
                inputMode="numeric"
                value={draft.startUniverse}
                placeholder="Automatic"
                aria-invalid={!!problems.startUniverse}
                aria-describedby={described("startUniverse")}
                onChange={(e) => set({ startUniverse: e.target.value })}
              />
            </Field>
          ) : (
            <p className="self-end pb-2 text-xs text-neutral-500">DDP needs no universes: channels follow the wiring.</p>
          )}
        </fieldset>
      </div>
      {draft.protocol === "sacn" && (
        <More id="controller-edit" label="More: universe size and multicast" forceOpen={multicast || !!problems.universeSize}>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <Field
              label="Channels per universe"
              id={id("universeSize")}
              problem={problems.universeSize}
              hint={`Any number from 1 to ${MAX_UNIVERSE_SIZE}. 510 fits exactly 170 RGB pixels in each universe. Match what the controller is set to.`}
            >
              <Input
                id={id("universeSize")}
                inputMode="numeric"
                value={draft.universeSize}
                aria-invalid={!!problems.universeSize}
                aria-describedby={described("universeSize")}
                onChange={(e) => set({ universeSize: e.target.value })}
              />
            </Field>
            <label className="flex items-center gap-2 self-end pb-2 text-sm">
              <input type="checkbox" checked={draft.multicast} onChange={(e) => set({ multicast: e.target.checked })} className="accent-accent-500" />
              Multicast
            </label>
          </div>
        </More>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <Button type="submit" variant="primary" disabled={!ok}>
          Save
        </Button>
        <Button variant="ghost" onClick={onDone}>
          Cancel
        </Button>
        {ok ? (
          <span className="text-xs text-neutral-500">
            The wiring stays as it is.
            {controller.sequenceChannels && " So does where it sits in FPP sequences."}
            {draft.protocol === "sacn" && ` Universes go from 1 to ${MAX_UNIVERSE.toLocaleString("en-US")}.`}
          </span>
        ) : (
          <span role="status" className="text-xs text-red-600 dark:text-red-400">
            To save, fix the {blocking.join(" and ")} above.
          </span>
        )}
      </div>
    </form>
  );
}
