import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { floorGrid, v3 } from "../../lib/layout3d";
import { VIEW_DEFAULTS } from "../../lib/viewOptions";
import { useView3d } from "../../state/view3d";
import { FlatViewControls, View3dControls } from "./View3dControls";
import { ViewOptionsControl } from "./ViewOptions";

const button = () => screen.getByRole("button", { name: "View" });
const panel = () => screen.getByRole("dialog", { name: "View" });
const group = (name: string) => within(panel()).getByRole("group", { name });

describe("the View options", () => {
  beforeEach(() => {
    localStorage.clear();
    useView3d.setState({ ...VIEW_DEFAULTS, floor: null });
  });

  it("is one small button that opens every option, each named, at the usual look to start with", async () => {
    const user = userEvent.setup();
    render(<ViewOptionsControl />);
    expect(button()).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.click(button());
    expect(button()).toHaveAttribute("aria-expanded", "true");
    expect(within(panel()).getByRole("checkbox", { name: "Prop names" })).not.toBeChecked();
    expect(within(group("Pixels that are off")).getByRole("radio", { name: "Dim" })).toBeChecked();
    expect(within(group("Dot size")).getByRole("radio", { name: "Medium" })).toBeChecked();
    expect(within(group("Boost dim")).getByRole("radio", { name: "Off" })).toBeChecked();
    // The flat views have no grid.
    expect(within(panel()).queryByRole("checkbox", { name: /Grid/ })).not.toBeInTheDocument();
    // The first option has the focus.
    expect(within(panel()).getByRole("checkbox", { name: "Prop names" })).toHaveFocus();
  });

  it("changes every preview's look straight away, remembered on this computer", async () => {
    const user = userEvent.setup();
    render(<ViewOptionsControl />);
    await user.click(button());
    await user.click(within(panel()).getByRole("checkbox", { name: "Prop names" }));
    await user.click(within(group("Pixels that are off")).getByRole("radio", { name: "Hidden" }));
    await user.click(within(group("Dot size")).getByRole("radio", { name: "Large" }));
    await user.click(within(group("Boost dim")).getByRole("radio", { name: "Medium" }));
    expect(useView3d.getState()).toMatchObject({ names: true, unlit: "hidden", dotSize: "large", boost: 2 });
    expect(JSON.parse(localStorage.getItem("pixelflow.view3dOptions")!)).toMatchObject({ names: true, unlit: "hidden", dotSize: "large", boost: 2 });
    // Changed from the usual: the button says so.
    expect(button().className).toContain("accent");
  });

  it("says what each choice of pixels that are off does", async () => {
    const user = userEvent.setup();
    render(<ViewOptionsControl />);
    await user.click(button());
    expect(panel()).toHaveTextContent("Shown dim, so every prop can be seen.");
    await user.click(within(group("Pixels that are off")).getByRole("radio", { name: "Hidden" }));
    expect(panel()).toHaveTextContent("Only lit pixels show, as at night.");
    await user.click(within(group("Pixels that are off")).getByRole("radio", { name: "Auto" }));
    expect(panel()).toHaveTextContent("Hidden while the show plays; dim while it's paused or stopped, and on the Layout screen.");
    expect(panel()).toHaveTextContent("Your lights still get the show's own colors.");
  });

  it("works from the keyboard: arrow keys move along a row of choices, Escape closes it and gives the focus back", async () => {
    const user = userEvent.setup();
    render(<ViewOptionsControl />);
    button().focus();
    await user.keyboard("{Enter}");
    expect(panel()).toBeInTheDocument();
    const dim = within(group("Pixels that are off")).getByRole("radio", { name: "Dim" });
    dim.focus();
    await user.keyboard("{ArrowRight}");
    expect(useView3d.getState().unlit).toBe("hidden");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(button()).toHaveFocus();
  });

  it("closes when the focus moves on out of it", async () => {
    const user = userEvent.setup();
    render(
      <>
        <ViewOptionsControl />
        <button type="button">Next</button>
      </>,
    );
    await user.click(button());
    // Clicking its words keeps it open.
    await user.click(within(panel()).getByText("Dot size"));
    expect(panel()).toBeInTheDocument();
    act(() => within(group("Dot size")).getByRole("radio", { name: "Medium" }).focus());
    act(() => screen.getByRole("button", { name: "Next" }).focus());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("closes with a click anywhere else, or on its button", async () => {
    const user = userEvent.setup();
    render(
      <>
        <ViewOptionsControl />
        <p>The preview</p>
      </>,
    );
    await user.click(button());
    await user.click(within(panel()).getByRole("checkbox", { name: "Prop names" }));
    expect(panel()).toBeInTheDocument();
    await user.click(screen.getByText("The preview"));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.click(button());
    await user.click(button());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("keeps its name where there's only room for its icon", () => {
    render(<ViewOptionsControl iconOnly />);
    expect(button().querySelector("span")).toHaveClass("sr-only");
  });

  it("is on the 3D view's own bar with the grid, which says how far apart its lines are", async () => {
    const user = userEvent.setup();
    render(<View3dControls />);
    const bar = screen.getByRole("toolbar", { name: "3D view" });
    await user.click(within(bar).getByRole("button", { name: "View" }));
    const grid = within(panel()).getByRole("checkbox", { name: /Grid on the ground/ });
    expect(grid).toBeChecked();
    act(() => useView3d.setState({ floor: floorGrid({ min: v3(-12, 0, 0), max: v3(12, 4, 0) }) }));
    expect(panel()).toHaveTextContent("Lines every 1 unit, heavier every 5 units.");
    await user.click(grid);
    expect(useView3d.getState().grid).toBe(false);
    // The ground keeps its own button.
    expect(within(bar).getByRole("button", { name: "Ground" })).toHaveAttribute("aria-pressed", "true");
  });

  it("is on the flat view's own bar too", () => {
    render(<FlatViewControls />);
    expect(within(screen.getByRole("toolbar", { name: "2D view" })).getByRole("button", { name: "View" })).toBeInTheDocument();
  });
});
