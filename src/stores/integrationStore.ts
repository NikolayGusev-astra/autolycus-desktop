import { create } from "zustand";
import { integrationService, type ConfigureIntegrationRequest, type IntegrationDefinition, type IntegrationEventEnvelope, type IntegrationTestResult, type IntegrationView } from "@/services/integrationService";

interface IntegrationState {
  availableDefinitions: IntegrationDefinition[];
  instances: Map<string, IntegrationView>;
  pendingInteractions: Map<string, string>;
  eventRevisions: Map<string, number>;
  loading: boolean;
  error: string | null;
  loadAvailable: () => Promise<void>;
  loadConfigured: () => Promise<void>;
  configure: (request: ConfigureIntegrationRequest) => Promise<IntegrationView>;
  enable: (id: string) => Promise<IntegrationView>;
  disable: (id: string) => Promise<IntegrationView>;
  test: (id: string) => Promise<IntegrationTestResult>;
  remove: (id: string) => Promise<void>;
  refreshStatus: (id: string) => Promise<IntegrationView>;
  handleEvent: (event: IntegrationEventEnvelope) => void;
  clearSecretInput: () => void;
}

const safeError = () => "We couldn't complete that integration action. Please try again.";
const withInstance = (view: IntegrationView, instances: Map<string, IntegrationView>) => new Map(instances).set(view.data.id, view);

export const useIntegrationStore = create<IntegrationState>()((set, get) => {
  const run = async <T,>(action: () => Promise<T>): Promise<T> => {
    set({ loading: true, error: null });
    try { return await action(); } catch (error) { set({ error: safeError() }); throw error; } finally { set({ loading: false }); }
  };
  const update = (view: IntegrationView) => set((state) => ({ instances: withInstance(view, state.instances) }));
  return {
    availableDefinitions: [], instances: new Map(), pendingInteractions: new Map(), eventRevisions: new Map(), loading: false, error: null,
    loadAvailable: () => run(async () => { const availableDefinitions = await integrationService.listAvailable(); set({ availableDefinitions }); }),
    loadConfigured: () => run(async () => { const views = await integrationService.listConfigured(); set({ instances: new Map(views.map((view) => [view.data.id, view])) }); }),
    configure: (request) => run(async () => { const view = await integrationService.configure(request); update(view); return view; }),
    enable: (id) => run(async () => { const view = await integrationService.enable(id); update(view); return view; }),
    disable: (id) => run(async () => { const view = await integrationService.disable(id); update(view); return view; }),
    test: (id) => run(() => integrationService.test(id)),
    remove: (id) => run(async () => { await integrationService.remove(id); set((state) => { const instances = new Map(state.instances); instances.delete(id); return { instances }; }); }),
    refreshStatus: (id) => run(async () => { const view = await integrationService.refreshStatus(id); update(view); return view; }),
    handleEvent: (envelope) => {
      if (envelope.revision <= (get().eventRevisions.get(envelope.instance_id) ?? 0)) return;
      set((state) => {
        const eventRevisions = new Map(state.eventRevisions).set(envelope.instance_id, envelope.revision);
        const instances = new Map(state.instances);
        if (envelope.event.kind === "removed") instances.delete(envelope.instance_id);
        return { eventRevisions, instances };
      });
      if (envelope.event.kind !== "removed") void get().refreshStatus(envelope.instance_id).catch(() => undefined);
    },
    clearSecretInput: () => undefined,
  };
});
