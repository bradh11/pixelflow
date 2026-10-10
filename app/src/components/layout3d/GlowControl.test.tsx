import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { useView3d } from "../../state/view3d";
import { GlowControl } from "./GlowControl";
import { View3dControls } from "./View3dControls";

const button = () => screen.getByRole("button", { name: "Glow" });
const slider = () => screen.getByRole("slider", { name: "Glow" });

describe("the Glow control", () => {
  it("is a small button that opens a slider from None to 100% in steps of 5", async () => {
    const user = userEvent.setup();
    render(<GlowControl />);
    expect(button()).toHaveAttribute("aria-expanded", "false");
    expect(button()).toHaveAttribute("title", "Glow around lit pixels: None");
    expect(screen.queryByRole("slider")).not.toBeInTheDocument();

    await user.click(button());
    expect(button()).toHaveAttribute("aria-expanded", "true");
    const panel = screen.getByRole("dialog", { name: "Glow" });
    expect(slider()).toHaveFocus();
    expect(slider()).toHaveValue("0");
    expect(slider()).toHaveAttribute("min", "0");
    expect(slider()).toHaveAttribute("max", "100");
    expect(slider()).toHaveAttribute("step", "5");
    expect(slider()).toHaveAttribute("aria-valuetext", "None");
    expect(panel).toHaveTextContent("None for bare bulbs, more for lights behind diffusers.");
  });

  it("sets how much every preview glows, straight away, and it's remembered on this computer", async () => {
    const user = userEvent.setup();
    render(<GlowControl />);
    await user.click(button());
    fireEvent.change(slider(), { target: { value: "45" } });
    expect(useView3d.getState().glow).toBe(0.45);
    expect(slider()).toHaveValue("45");
    expect(slider()).toHaveAttribute("aria-valuetext", "45%");
    expect(screen.getByRole("dialog", { name: "Glow" })).toHaveTextContent("45%");
    expect(button()).toHaveAttribute("title", "Glow around lit pixels: 45%");
    expect(JSON.parse(localStorage.getItem("pixelflow.view3dOptions")!)).toEqual({ glow: 0.45, ground: true });
    fireEvent.change(slider(), { target: { value: "0" } });
    expect(useView3d.getState().glow).toBe(0);
    expect(slider()).toHaveAttribute("aria-valuetext", "None");
  });

  it("shows the level another preview's control set", () => {
    useView3d.getState().setGlow(0.8);
    render(<GlowControl />);
    fireEvent.click(button());
    expect(slider()).toHaveValue("80");
  });

  it("closes with Escape, giving the focus back, or with a click anywhere else", async () => {
    const user = userEvent.setup();
    render(
      <>
        <GlowControl />
        <p>The preview</p>
      </>,
    );
    await user.click(button());
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(button()).toHaveFocus();
    await user.click(button());
    expect(screen.getByRole("dialog", { name: "Glow" })).toBeInTheDocument();
    await user.click(screen.getByText("The preview"));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    // The button itself opens and closes it.
    await user.click(button());
    await user.click(button());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("keeps its name where there's only room for its icon", () => {
    render(<GlowControl iconOnly />);
    expect(button()).toBeInTheDocument();
    expect(button().querySelector("span")).toHaveClass("sr-only");
  });

  it("is the only Glow control: the 3D view's own tool bar has none", () => {
    render(<View3dControls />);
    const bar = screen.getByRole("toolbar", { name: "3D view" });
    expect(bar).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Glow" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Ground" })).toBeInTheDocument();
  });
});
