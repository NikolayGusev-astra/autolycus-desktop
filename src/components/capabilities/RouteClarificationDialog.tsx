import type { CapabilityRouteClarificationEvent } from "@/services/capabilityRouter";

export function RouteClarificationDialog({ event, onChoose, onClose }: {
  event: CapabilityRouteClarificationEvent;
  onChoose: (instanceId: string) => void;
  onClose: () => void;
}) {
  const { clarification } = event;
  return <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
    <section role="dialog" aria-modal="true" aria-labelledby="capability-route-title" className="w-full max-w-md rounded-lg border border-ac-border bg-ac-bg p-4 shadow-xl">
      <h2 id="capability-route-title" className="text-sm font-semibold text-ac-ink">Choose an integration</h2>
      <p className="mt-2 text-sm text-ac-muted">{clarification.prompt}</p>
      <div className="mt-4 space-y-2">
        {clarification.choices.map((choice) => <button key={choice.instance_id} type="button" onClick={() => onChoose(choice.instance_id)} className="block w-full rounded border border-ac-border px-3 py-2 text-left hover:bg-ac-surface">
          <span className="block text-sm font-medium text-ac-ink">{choice.label}</span>
          <span className="block text-xs text-ac-muted">{choice.description}</span>
        </button>)}
      </div>
      <div className="mt-4 flex justify-end"><button type="button" onClick={onClose} className="px-3 py-1.5 text-xs text-ac-muted">Cancel</button></div>
    </section>
  </div>;
}
