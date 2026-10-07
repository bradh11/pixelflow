// Whether one of the shell's file dialogs is showing. While a sheet is up, macOS still passes
// ⌘-keys and clicks to the menu bar; the window holds them back until the dialog is answered.

let showing = 0;

/** True while a file dialog the window asked for hasn't been answered. */
export function fileDialogShowing(): boolean {
  return showing > 0;
}

/** Runs `ask` (a call that shows a file dialog), counting the dialog as showing until it's answered. */
export async function whileFileDialog<T>(ask: () => Promise<T>): Promise<T> {
  showing++;
  try {
    return await ask();
  } finally {
    showing--;
  }
}
