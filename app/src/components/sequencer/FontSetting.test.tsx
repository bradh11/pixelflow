import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MEMORY_FONTS, MemorySequencer } from "../../api/memorySequencer";
import { useSequencer } from "../../state/sequencer";
import { FontSetting, PIXEL_FONT, installedFont } from "./FontSetting";

beforeEach(() => {
  // A fresh stand-in each time, so the font list is asked for again.
  useSequencer.setState({ api: new MemorySequencer() });
});

function picker(value: string, styles?: { bold?: boolean; italic?: boolean }) {
  const onChange = vi.fn();
  const onBold = vi.fn();
  const onItalic = vi.fn();
  render(
    <FontSetting
      label="Font"
      description="The font."
      value={value}
      onChange={onChange}
      styles={styles && { bold: styles.bold ?? false, italic: styles.italic ?? false, onBold, onItalic }}
    />,
  );
  return { onChange, onBold, onItalic, input: screen.getByRole("combobox", { name: "Font" }) };
}

describe("a Text effect's font", () => {
  it("lists the pixel font first, then the computer's fonts, and narrows the list as you type", async () => {
    const user = userEvent.setup();
    const { input, onChange } = picker("");
    expect(input).toHaveValue(PIXEL_FONT);
    await user.click(input);
    const list = await screen.findByRole("listbox", { name: "Fonts" });
    await waitFor(() => expect(within(list).getAllByRole("option")).toHaveLength(MEMORY_FONTS.length + 1));
    expect(within(list).getAllByRole("option")[0]).toHaveTextContent(PIXEL_FONT);
    expect(within(list).getByRole("option", { name: PIXEL_FONT })).toHaveAttribute("aria-selected", "true");
    await user.type(input, "ver");
    expect(within(list).getAllByRole("option").map((o) => o.textContent)).toEqual(["Verdana"]);
    await user.keyboard("{Enter}");
    expect(onChange).toHaveBeenCalledWith("Verdana");
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("goes back to the pixel font, and says when nothing is called what was typed", async () => {
    const user = userEvent.setup();
    const { input, onChange } = picker("Georgia");
    await waitFor(() => expect(input).toHaveValue("Georgia"));
    await user.click(input);
    await user.type(input, "zzz");
    expect(screen.getByText("No font on this computer is called that.")).toBeInTheDocument();
    await user.clear(input);
    await user.click(await screen.findByRole("option", { name: PIXEL_FONT }));
    expect(onChange).toHaveBeenCalledWith("");
    // Escape closes the list and changes nothing.
    await user.click(input);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it("says plainly when the font isn't on this computer", async () => {
    const { input } = picker("Arial Narrow");
    expect(await screen.findByText("Arial Narrow isn't on this computer: shown in PixelFlow's pixel font.")).toBeInTheDocument();
    expect(input).toHaveValue("Arial Narrow");
    // Any case of a font it has is that font, spelled as the computer spells it.
    expect(installedFont(MEMORY_FONTS, "times new roman")).toBe("Times New Roman");
  });

  it("puts bold and italic beside it, for a computer font only", async () => {
    const user = userEvent.setup();
    picker("", { bold: false, italic: false });
    expect(screen.getByRole("button", { name: "Bold" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Italic" })).toBeDisabled();
    cleanup();
    const { onBold, onItalic } = picker("Arial", { bold: true, italic: false });
    const bold = screen.getByRole("button", { name: "Bold" });
    expect(bold).toHaveAttribute("aria-pressed", "true");
    await user.click(bold);
    expect(onBold).toHaveBeenCalledWith(false);
    await user.click(screen.getByRole("button", { name: "Italic" }));
    expect(onItalic).toHaveBeenCalledWith(true);
  });
});
