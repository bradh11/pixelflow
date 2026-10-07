import { Keyboard, Sparkles } from "lucide-react";
import type { ReactNode } from "react";
import { version } from "../../package.json";
import { providerName } from "../api/assistant";
import { Button, Card, PageHeader } from "../components/ui";
import { hintFor } from "../lib/shortcuts";
import { useAssistant } from "../state/assistant";
import { GRID_SIZES, useLayoutEditor } from "../state/layoutEditor";
import { useSequencer } from "../state/sequencer";
import { useShortcutSheet } from "../state/shortcutSheet";
import { type ThemeChoice, useApp } from "../state/store";

const THEMES: { value: ThemeChoice; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

function Section({ title, description, children }: { title: string; description?: string; children: ReactNode }) {
  return (
    <Card className="flex flex-col gap-3">
      <div>
        <h2 className="font-semibold">{title}</h2>
        {description && <p className="mt-0.5 text-sm text-neutral-500 dark:text-neutral-400">{description}</p>}
      </div>
      {children}
    </Card>
  );
}

function Toggle({ label, hint, checked, onChange }: { label: string; hint: string; checked: boolean; onChange: (on: boolean) => void }) {
  return (
    <label className="flex items-start gap-2 text-sm">
      <input type="checkbox" className="mt-0.5 accent-accent-500" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span>
        {label}
        <span className="block text-xs text-neutral-500">{hint}</span>
      </span>
    </label>
  );
}

/**
 * Settings for this computer: the theme, the AI provider, playback, and the layout editor. Each is
 * kept where it already lives (on this computer, or with the AI key in the credential store), and
 * applies at once.
 */
export function SettingsScreen() {
  const themeChoice = useApp((s) => s.themeChoice);
  const setTheme = useApp((s) => s.setTheme);
  const provider = useAssistant((s) => s.provider);
  const hasKey = useAssistant((s) => s.hasKey);
  const looping = useSequencer((s) => s.looping);
  const grid = useLayoutEditor((s) => s.grid);
  const smartGuides = useLayoutEditor((s) => s.smartGuides);
  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-4">
      <PageHeader title="Settings" description="Kept on this computer, for every show." />

      <Section title="Appearance">
        <fieldset className="flex flex-col gap-2">
          <legend className="mb-1 text-sm text-neutral-600 dark:text-neutral-400">Theme</legend>
          <div className="flex gap-2">
            {THEMES.map((t) => (
              <label
                key={t.value}
                className={`flex cursor-pointer items-center gap-2 rounded-md border px-3 py-1.5 text-sm ${
                  themeChoice === t.value
                    ? "border-accent-500 bg-accent-50 text-accent-700 dark:bg-accent-600/15 dark:text-accent-300"
                    : "border-neutral-300 hover:bg-neutral-100 dark:border-neutral-700 dark:hover:bg-neutral-800"
                }`}
              >
                <input type="radio" name="theme" value={t.value} checked={themeChoice === t.value} onChange={() => setTheme(t.value)} className="accent-accent-500" />
                {t.label}
              </label>
            ))}
          </div>
          <p className="text-xs text-neutral-500">System follows this computer's light or dark setting.</p>
        </fieldset>
      </Section>

      <Section title="AI" description="The assistant's provider, API key and model.">
        <p className="text-sm">
          {providerName(provider)}
          {" · "}
          <span className="text-neutral-500">{hasKey ? "Key saved" : hasKey === false ? "No key yet" : "Checking the key…"}</span>
        </p>
        <div>
          <Button onClick={() => useAssistant.getState().setSettingsOpen(true)}>
            <Sparkles size={14} aria-hidden /> AI settings…
          </Button>
        </div>
      </Section>

      <Section title="Playback">
        <Toggle
          label="Loop sequences while editing"
          hint="Playback on the Sequence screen starts again from the top when it reaches the end (L)."
          checked={looping}
          onChange={(on) => useSequencer.getState().setLooping(on)}
        />
        <p className="text-xs text-neutral-500">Each sequence's audio offset is set beside it on the Play screen.</p>
      </Section>

      <Section title="Layout">
        <label className="flex flex-col gap-1 text-sm">
          <span className="text-neutral-600 dark:text-neutral-400">Grid spacing (used when Snap to grid is on)</span>
          <select
            value={grid}
            onChange={(e) => useLayoutEditor.getState().setGrid(Number(e.target.value))}
            className="w-40 rounded-md border border-neutral-300 bg-white px-2 py-1.5 text-sm dark:border-neutral-700 dark:bg-neutral-950"
          >
            {GRID_SIZES.map((g) => (
              <option key={g} value={g}>
                {g} {g === 1 ? "unit" : "units"}
              </option>
            ))}
          </select>
        </label>
        <Toggle
          label="Smart guides"
          hint="Props snap to line up with, space evenly from, and match the size of others as you move and draw them (hold Option or Alt to place freely)."
          checked={smartGuides}
          onChange={(on) => useLayoutEditor.getState().setSmartGuides(on)}
        />
      </Section>

      <Section title="About">
        <p className="text-sm">PixelFlow {version}</p>
        <div>
          <Button onClick={() => useShortcutSheet.getState().setOpen(true)}>
            <Keyboard size={14} aria-hidden /> Keyboard shortcuts <kbd className="font-sans text-xs text-neutral-400">{hintFor("shortcuts")}</kbd>
          </Button>
        </div>
      </Section>
    </div>
  );
}
