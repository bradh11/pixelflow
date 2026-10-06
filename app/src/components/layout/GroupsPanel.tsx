import { ChevronDown, ChevronRight, GripVertical, Group as GroupIcon, Plus, Trash2, X } from "lucide-react";
import { type KeyboardEvent, type PointerEvent, useEffect, useMemo, useRef, useState } from "react";
import type { Group, GroupMember, Show } from "../../api/types";
import { plural } from "../../lib/format";
import {
  addMembersEdits,
  groupPropIds,
  memberKey,
  memberLabel,
  moveMemberEdits,
  removeGroupEdits,
  removeMemberEdits,
  renameGroupEdits,
} from "../../lib/groupEdits";
import { submodelsOf } from "../../lib/submodels";
import { groupSelected } from "../../state/groups";
import { useLayoutEditor } from "../../state/layoutEditor";
import { useApp } from "../../state/store";
import { useSequencer } from "../../state/sequencer";
import { toastWithUndo } from "../../state/undoToast";
import { deleteUseWarning } from "../../lib/sequenceUse";
import { Button, Input, Select } from "../ui";

/** The group's name, saved on Enter or leaving the field (an empty name goes back). */
function NameField({ group }: { group: Group }) {
  const apply = useApp((s) => s.apply);
  const [name, setName] = useState(group.name);
  useEffect(() => setName(group.name), [group.name]);
  return (
    <label className="flex flex-col gap-1 text-xs">
      <span className="text-neutral-500">Group name</span>
      <Input
        value={name}
        onChange={(e) => setName(e.target.value)}
        onBlur={() => {
          if (name.trim()) void apply(renameGroupEdits(group.id, name));
          else setName(group.name);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") {
            setName(group.name);
            e.stopPropagation();
          }
        }}
      />
    </label>
  );
}

/** "Add a prop or submodel…": every prop and submodel not in the group yet. */
function AddMemberPicker({ show, group }: { show: Show; group: Group }) {
  const apply = useApp((s) => s.apply);
  const inGroup = new Set(group.members.map(memberKey));
  return (
    <Select
      aria-label={`Add a prop or submodel to ${group.name}`}
      value=""
      onChange={(e) => {
        const [prop, region] = e.target.value.split("/");
        if (!prop) return;
        const member: GroupMember = region ? { prop, region } : prop;
        void apply(addMembersEdits(group.id, [member]));
      }}
      className="w-full text-xs"
    >
      <option value="">Add a prop or submodel…</option>
      {show.props.map((p) => {
        const subs = submodelsOf(p).filter((r) => !inGroup.has(`${p.id}/${r.id}`));
        if (inGroup.has(p.id) && subs.length === 0) return null;
        return (
          <optgroup key={p.id} label={p.name}>
            {!inGroup.has(p.id) && <option value={p.id}>{p.name}</option>}
            {subs.map((r) => (
              <option key={r.id} value={`${p.id}/${r.id}`}>
                {p.name} / {r.name}
              </option>
            ))}
          </optgroup>
        );
      })}
    </Select>
  );
}

/**
 * The group's members in order (effects that run along the group, like chases, follow it). Drag a
 * member's handle to move it, or press Alt (Option) with the up and down arrows on it.
 */
function MemberList({ show, group }: { show: Show; group: Group }) {
  const apply = useApp((s) => s.apply);
  const list = useRef<HTMLOListElement>(null);
  const [drag, setDrag] = useState<{ from: number; over: number } | null>(null);
  const [focusKey, setFocusKey] = useState<string | null>(null);

  // After a keyboard move, keep the focus on the member that moved.
  useEffect(() => {
    if (!focusKey) return;
    list.current?.querySelector<HTMLElement>(`[data-member="${CSS.escape(focusKey)}"]`)?.focus();
  }, [group.members, focusKey]);

  /** Where a pointer at `y` would drop: before the member whose middle is below it. */
  const dropIndex = (y: number) => {
    const items = [...(list.current?.querySelectorAll<HTMLElement>("li") ?? [])];
    const at = items.findIndex((li) => {
      const r = li.getBoundingClientRect();
      return y < r.top + r.height / 2;
    });
    return at < 0 ? items.length : at;
  };
  const move = (member: GroupMember, from: number, before: number) => {
    const to = before > from ? before - 1 : before;
    if (to !== from) void apply(moveMemberEdits(group.id, member, to));
  };

  if (group.members.length === 0) {
    return <p className="rounded border border-dashed border-neutral-300 p-2 text-center text-xs text-neutral-500 dark:border-neutral-700">No members yet. Add the selected props, or pick one below.</p>;
  }
  return (
    <>
      <p id="group-member-help" className="sr-only">
        Drag to change the order, or press Alt with the up and down arrows.
      </p>
      <ol ref={list} aria-label={`Members of ${group.name}, in order`} className="flex max-h-72 flex-col overflow-auto rounded border border-neutral-200 dark:border-neutral-800">
        {group.members.map((m, i) => {
          const label = memberLabel(show, m);
          const key = memberKey(m);
          const onKeyDown = (e: KeyboardEvent) => {
            if (!e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
            e.preventDefault();
            e.stopPropagation();
            setFocusKey(key);
            move(m, i, e.key === "ArrowUp" ? i - 1 : i + 2);
          };
          const handlers = {
            onPointerDown: (e: PointerEvent<HTMLButtonElement>) => {
              if (e.button !== 0) return;
              e.currentTarget.setPointerCapture?.(e.pointerId);
              setDrag({ from: i, over: i });
            },
            onPointerMove: (e: PointerEvent<HTMLButtonElement>) => {
              if (drag) setDrag({ ...drag, over: dropIndex(e.clientY) });
            },
            onPointerUp: (e: PointerEvent<HTMLButtonElement>) => {
              if (!drag) return;
              setDrag(null);
              move(m, drag.from, dropIndex(e.clientY));
            },
            onPointerCancel: () => setDrag(null),
          };
          return (
            <li
              key={key}
              className={`relative flex items-center gap-1 border-t border-neutral-200 px-1 py-0.5 text-sm first:border-t-0 dark:border-neutral-800 ${drag?.from === i ? "opacity-50" : ""}`}
            >
              {drag && drag.over === i && drag.from !== i && <span aria-hidden className="absolute top-0 right-0 left-0 h-0.5 bg-accent-500" />}
              <span className="w-6 shrink-0 text-right text-[10px] text-neutral-400 tabular-nums">{i + 1}</span>
              <button
                type="button"
                data-member={key}
                aria-label={`Move ${label}`}
                aria-describedby="group-member-help"
                title="Drag to change the order (or Alt + up/down arrows)"
                className="shrink-0 cursor-grab touch-none rounded p-0.5 text-neutral-400 hover:text-neutral-700 dark:hover:text-neutral-200"
                onKeyDown={onKeyDown}
                {...handlers}
              >
                <GripVertical size={14} aria-hidden />
              </button>
              <span className="min-w-0 flex-1 truncate" title={label}>
                {label}
              </span>
              <button
                type="button"
                aria-label={`Remove ${label} from ${group.name}`}
                title={`Remove from ${group.name}`}
                className="shrink-0 rounded p-0.5 text-neutral-400 hover:text-red-600 dark:hover:text-red-400"
                onClick={() => void apply(removeMemberEdits(group.id, m))}
              >
                <X size={13} />
              </button>
            </li>
          );
        })}
        {drag && drag.over === group.members.length && <li aria-hidden className="h-0.5 bg-accent-500" />}
      </ol>
    </>
  );
}

function GroupEditor({ show, group }: { show: Show; group: Group }) {
  const apply = useApp((s) => s.apply);
  const edit = useApp((s) => s.edit);
  const selected = useLayoutEditor((s) => s.selected);
  const inGroup = new Set(group.members.map(memberKey));
  const toAdd = selected.filter((id) => !inGroup.has(id) && show.props.some((p) => p.id === id));
  const [asking, setAsking] = useState<string | null>(null);
  const remove = async () => {
    setAsking(null);
    useLayoutEditor.getState().setSidePanel({ group: null });
    toastWithUndo(`Deleted ${group.name}`, await edit(removeGroupEdits(group.id)));
  };
  return (
    <div className="flex flex-col gap-2 border-t border-neutral-200 bg-neutral-50 p-2 dark:border-neutral-800 dark:bg-neutral-950/50">
      <NameField group={group} />
      <MemberList show={show} group={group} />
      <Button variant="secondary" className="w-full text-xs" disabled={toAdd.length === 0} onClick={() => void apply(addMembersEdits(group.id, toAdd))} title="Add the props selected on the canvas, at the end">
        <Plus size={14} /> Add selected {toAdd.length > 0 ? `(${toAdd.length})` : "props"}
      </Button>
      <AddMemberPicker show={show} group={group} />
      {asking ? (
        <div role="alert" className="flex flex-col gap-2 rounded border border-amber-300 bg-amber-50 p-2 text-xs text-amber-900 dark:border-amber-800 dark:bg-amber-950/40 dark:text-amber-200">
          <p>{asking}</p>
          <div className="flex gap-2">
            <Button variant="danger" className="text-xs" onClick={() => void remove()}>
              <Trash2 size={14} aria-hidden /> Delete anyway
            </Button>
            <Button className="text-xs" onClick={() => setAsking(null)}>
              Keep it
            </Button>
          </div>
        </div>
      ) : (
        <Button
          variant="danger"
          className="w-full text-xs"
          onClick={() => {
            // Rows in the open sequence that light this group would be left showing nothing.
            const warning = deleteUseWarning(useSequencer.getState().doc, `Group “${group.name}”`, { group: group.id });
            if (warning) setAsking(warning);
            else void remove();
          }}
        >
          <Trash2 size={14} /> Delete group
        </Button>
      )}
    </div>
  );
}

/**
 * Groups of props (and submodels), as xLights' model groups: one sequencer row lights them as one
 * picture, in member order. Picking a group selects its props on the canvas.
 */
export function GroupsPanel() {
  const show = useApp((s) => s.snapshot!.show);
  const selected = useLayoutEditor((s) => s.selected.length);
  const open = useLayoutEditor((s) => s.sidePanel.group);
  // How many rows of the open sequence light each group (shown, and asked about before deleting).
  const doc = useSequencer((s) => s.doc);
  const used = useMemo(() => {
    const counts = new Map<string, number>();
    for (const row of doc?.rows ?? []) if ("group" in row.target) counts.set(row.target.group, (counts.get(row.target.group) ?? 0) + 1);
    return counts;
  }, [doc]);
  const setSidePanel = useLayoutEditor((s) => s.setSidePanel);
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex flex-col gap-1.5 border-b border-neutral-200 p-2 dark:border-neutral-800">
        <Button variant="primary" className="w-full" disabled={selected === 0} onClick={() => void groupSelected()} title="Make a group of the selected props, in the order you picked them (⌘G)">
          <GroupIcon size={15} aria-hidden /> Group selected{selected > 0 ? ` (${selected})` : ""}
        </Button>
        <p className="text-xs text-neutral-500">{selected === 0 ? "Select props on the canvas or in the Props list, then group them (⌘G)." : "Shortcut: ⌘G. The order you pick them in is the group's order."}</p>
      </div>
      {show.groups.length === 0 ? (
        <p className="p-4 text-center text-sm text-neutral-500">No groups yet.</p>
      ) : (
        <ul aria-label="Groups" className="min-h-0 flex-1 overflow-auto">
          {show.groups.map((g) => {
            const expanded = open === g.id;
            return (
              <li key={g.id} className="border-b border-neutral-200 dark:border-neutral-800">
                <button
                  type="button"
                  aria-expanded={expanded}
                  title="Select this group's props, and edit the group"
                  className={`flex w-full items-center gap-1.5 px-2 py-1.5 text-left text-sm hover:bg-neutral-100 dark:hover:bg-neutral-800/70 ${expanded ? "font-medium" : ""}`}
                  onClick={() => {
                    useLayoutEditor.getState().select(groupPropIds(show, g));
                    setSidePanel({ group: expanded ? null : g.id });
                  }}
                >
                  {expanded ? <ChevronDown size={14} aria-hidden /> : <ChevronRight size={14} aria-hidden />}
                  <span className="min-w-0 flex-1 truncate">{g.name}</span>
                  <span className="shrink-0 text-right text-xs text-neutral-500 tabular-nums">
                    {plural(g.members.length, "member")}
                    {doc && (used.get(g.id) ?? 0) > 0 && (
                      <span className="block text-[10px] text-violet-700 dark:text-violet-300">
                        {plural(used.get(g.id)!, "row")} in {doc.name}
                      </span>
                    )}
                  </span>
                </button>
                {expanded && <GroupEditor show={show} group={g} />}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
