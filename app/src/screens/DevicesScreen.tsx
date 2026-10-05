import { EmptyState, PageHeader } from "../components/ui";

export function DevicesScreen() {
  return (
    <div className="mx-auto max-w-4xl">
      <PageHeader title="Devices" description="Find FPP and WLED controllers on your network and keep their settings in sync." />
      <EmptyState title="Device discovery is coming in a later update">
        For now, add controllers by IP address on the Wiring screen.
      </EmptyState>
    </div>
  );
}
