import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DmxChannel, Show } from "../../api/types";
import { DmxChannelsSetting, fixtureChannels } from "./DmxSettings";

const channels: DmxChannel[] = [
  { name: "Dimmer", role: "dimmer" },
  { name: "Red", role: "red" },
  { name: "Strobe", role: "strobe" },
  { name: "Mode", role: "fixed", value: 40 },
];

const animate = () => ({ curve: null, onChange: vi.fn(async () => true) });

describe("the DMX effect's channels", () => {
  it("names each channel set by the row's fixture, and sets another from the list", () => {
    const onChange = vi.fn(async () => true);
    render(<DmxChannelsSetting effectId="e" values={{ "3": 200 }} channels={channels} onChange={onChange} animate={animate} />);
    expect(screen.getByText("3 · Strobe")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Channel to set"), { target: { value: "4" } });
    fireEvent.click(screen.getByLabelText("Set this channel"));
    expect(onChange).toHaveBeenCalledWith(4, 40);
    fireEvent.click(screen.getByLabelText("Stop setting 3 · Strobe"));
    expect(onChange).toHaveBeenCalledWith(3, undefined);
  });

  it("finds the fixture of a prop's row or a group's", () => {
    const flood = { id: "f", name: "Flood", dmx: { address: 1, channels } };
    const show = { props: [{ id: "a", name: "Arch" }, flood], groups: [{ id: "g", name: "All", members: ["a", "f"] }] } as unknown as Show;
    expect(fixtureChannels(show, { prop: "f" })).toBe(channels);
    expect(fixtureChannels(show, { group: "g" })).toBe(channels);
    expect(fixtureChannels(show, { prop: "a" })).toEqual([]);
  });
});
