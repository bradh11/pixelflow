import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Button, IconButton, More, moreOpen } from "./ui";

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

describe("More", () => {
  const form = (id = "form") => (
    <More id={id} label="More: advanced">
      <label>
        Multicast <input type="checkbox" />
      </label>
    </More>
  );

  it("starts folded, opens and folds with its button, and says which", async () => {
    const user = userEvent.setup();
    render(form());
    const button = screen.getByRole("button", { name: "More: advanced" });
    expect(button).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByLabelText("Multicast")).not.toBeInTheDocument();
    await user.click(button);
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("group", { name: "More: advanced" })).toContainElement(screen.getByLabelText("Multicast"));
    await user.click(button);
    expect(screen.queryByLabelText("Multicast")).not.toBeInTheDocument();
  });

  it("opens the way it was last left, for that form only", async () => {
    const user = userEvent.setup();
    const { unmount } = render(form("controller"));
    await user.click(screen.getByRole("button", { name: "More: advanced" }));
    unmount();
    expect(moreOpen("controller")).toBe(true);
    expect(moreOpen("effect")).toBe(false);
    render(form("controller"));
    expect(screen.getByLabelText("Multicast")).toBeInTheDocument();
  });

  it("still works when storage can't be used", async () => {
    const user = userEvent.setup();
    const get = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    const set = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    render(form());
    await user.click(screen.getByRole("button", { name: "More: advanced" }));
    expect(screen.getByLabelText("Multicast")).toBeInTheDocument();
    get.mockRestore();
    set.mockRestore();
  });
});
