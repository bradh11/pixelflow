import { create } from "zustand";

/** A question that must be answered before something hard to take back happens. */
export interface ConfirmRequest {
  title: string;
  message: string;
  /** The button that goes ahead ("Delete anyway"). */
  confirm: string;
}

interface ConfirmState {
  request: ConfirmRequest | null;
  answer: ((yes: boolean) => void) | null;
  resolve(yes: boolean): void;
}

export const useConfirm = create<ConfirmState>((set, get) => ({
  request: null,
  answer: null,
  resolve(yes) {
    const answer = get().answer;
    set({ request: null, answer: null });
    answer?.(yes);
  },
}));

/** Asks (see `ConfirmDialog`); resolves true to go ahead. A new question cancels an open one. */
export function confirmAction(request: ConfirmRequest): Promise<boolean> {
  useConfirm.getState().answer?.(false);
  return new Promise((resolve) => useConfirm.setState({ request, answer: resolve }));
}
