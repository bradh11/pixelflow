import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Button, IconButton } from "./ui";

describe("Button", () => {
  it("gives an icon-only button its name as a tooltip", () => {
    render(<Button aria-label="New chat">+</Button>);
    expect(screen.getByRole("button", { name: "New chat" })).toHaveAttribute("data-tip", "New chat");
  });

  it("keeps a title or tip it was given", () => {
    render(
      <>
        <Button aria-label="Close" title="Close (⌘L)">
          x
        </Button>
        <Button aria-label="Open" data-tip="Open a show">
          o
        </Button>
      </>,
    );
    expect(screen.getByRole("button", { name: "Close" })).not.toHaveAttribute("data-tip");
    expect(screen.getByRole("button", { name: "Open" })).toHaveAttribute("data-tip", "Open a show");
  });
});

describe("IconButton", () => {
  it("is named by its label, with the label (or a longer hint) and shortcut as its tooltip", () => {
    render(
      <>
        <IconButton label="Undo" hint="Undo: Move Mega Tree" shortcut="⌘Z" onClick={() => undefined}>
          ↶
        </IconButton>
        <IconButton label="Zoom in" onClick={() => undefined}>
          +
        </IconButton>
      </>,
    );
    const undo = screen.getByRole("button", { name: "Undo" });
    expect(undo).toHaveAttribute("data-tip", "Undo: Move Mega Tree");
    expect(undo).toHaveAttribute("data-tip-key", "⌘Z");
    expect(screen.getByRole("button", { name: "Zoom in" })).toHaveAttribute("data-tip", "Zoom in");
  });
});
