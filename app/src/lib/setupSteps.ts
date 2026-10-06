// "Set up your show": the steps from an empty show to lights playing, each ticked from the show
// itself (or, for testing and sequencing, from what's been done with it).

import type { Show } from "../api/types";
import type { Screen } from "../state/store";
import { plural } from "./format";
import { needsWiring, wiringStatuses } from "./propList";
import { nodeCount } from "./shows";

export type SetupStepId = "controllers" | "props" | "wiring" | "test" | "sequence" | "play";

export interface SetupStep {
  id: SetupStepId;
  label: string;
  /** How far along it is ("2 props not wired"), or what to do. */
  detail: string;
  done: boolean;
  /** Where it's done. */
  screen: Screen;
}

export interface SetupDone {
  /** A test pattern has been sent for this show. */
  tested: boolean;
  /** A sequence has been made or opened for this show. */
  sequenced: boolean;
}

export function setupSteps(show: Show, done: SetupDone): SetupStep[] {
  const nodes = new Map(show.props.map((p) => [p.id, nodeCount(p.shape)]));
  const wiring = wiringStatuses(show, nodes);
  const unwired = show.props.filter((p) => needsWiring(wiring.get(p.id))).length;
  const hasProps = show.props.length > 0;
  return [
    {
      id: "props",
      label: "Draw your props",
      detail: hasProps ? plural(show.props.length, "prop") : "Over a photo of your house",
      done: hasProps,
      screen: "layout",
    },
    {
      id: "controllers",
      label: "Find your controllers",
      detail: show.controllers.length ? plural(show.controllers.length, "controller") : "Scan the network, or add one",
      done: show.controllers.length > 0,
      screen: "devices",
    },
    {
      id: "wiring",
      label: "Wire your props",
      detail: !hasProps ? "To controller ports" : unwired ? `${plural(unwired, "prop")} not wired` : "All wired",
      done: hasProps && unwired === 0,
      screen: "wiring",
    },
    {
      id: "test",
      label: "Test your lights",
      detail: done.tested ? "Tested" : "Light each prop to check it",
      done: done.tested,
      screen: "test",
    },
    {
      id: "sequence",
      label: "Make a sequence",
      detail: done.sequenced ? "Started" : "Effects timed to a song",
      done: done.sequenced,
      screen: "sequence",
    },
    {
      id: "play",
      label: "Add it to the playlist",
      detail: show.sequences.length ? `${plural(show.sequences.length, "sequence")} on the playlist` : "Ready to play on the Play screen",
      done: show.sequences.length > 0,
      screen: "play",
    },
  ];
}

/** The first step not done yet, or null when the show is set up. */
export function nextStep(steps: SetupStep[]): SetupStep | null {
  return steps.find((s) => !s.done) ?? null;
}
