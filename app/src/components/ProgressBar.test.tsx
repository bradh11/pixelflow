import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ProgressBar } from "./ProgressBar";

describe("ProgressBar", () => {
  it("shows how far the work has got, with its label", () => {
    render(<ProgressBar label="Reading the music" fraction={0.426} />);
    const bar = screen.getByRole("progressbar", { name: "Reading the music" });
    expect(bar).toHaveAttribute("aria-valuenow", "43");
    expect(bar).toHaveAttribute("aria-valuetext", "43%");
    expect(screen.getByText("43%")).toBeInTheDocument();
    expect(screen.getByText("Reading the music…")).toBeInTheDocument();
  });

  it("sweeps when it can't tell how far, and a slim bar has no text", () => {
    render(<ProgressBar slim label="Finding the beats" fraction={null} />);
    const bar = screen.getByRole("progressbar", { name: "Finding the beats" });
    expect(bar).not.toHaveAttribute("aria-valuenow");
    expect(bar).toHaveAttribute("aria-valuetext", "Finding the beats…");
    expect(screen.queryByText("Finding the beats…")).toBeNull();
  });

  it("keeps to 0–100%", () => {
    render(<ProgressBar label="Reading" fraction={1.7} />);
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "100");
  });
});
