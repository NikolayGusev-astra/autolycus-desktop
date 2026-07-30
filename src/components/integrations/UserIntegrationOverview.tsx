import { useEffect, useState } from "react";
import { useIntegrationStore } from "@/stores/integrationStore";
import { integrationService, type UserIntegrationDto } from "@/services/integrationService";
import { IntegrationStatusBadge, statusDescription } from "@/utils/integrationPresentation";

export function UserIntegrationCard({ integration, onOpen }: { integration: UserIntegrationDto; onOpen: (integration: UserIntegrationDto) => void }) {
  return <article className="rounded-lg border border-ac-border bg-ac-surface p-4"><div className="flex items-start justify-between gap-3"><div><h2 className="font-medium text-ac-ink">{integration.display_name}</h2><p className="text-xs text-ac-muted">{integration.category}</p></div><IntegrationStatusBadge status={integration.status} /></div><p className="mt-3 text-sm text-ac-muted">{integration.description}</p><p className="mt-3 text-xs text-ac-muted">{integration.capabilities.map((capability) => capability.display_name).join(", ") || "No capabilities available"}</p><button className="ac-btn mt-4 px-3 py-1.5 text-xs" onClick={() => onOpen(integration)}>{integration.enabled ? "Manage" : "Connect"}</button></article>;
}

export function UserIntegrationDetail({ integration, onBack }: { integration: UserIntegrationDto; onBack: () => void }) {
  const enable = useIntegrationStore((s) => s.enable); const disable = useIntegrationStore((s) => s.disable); const test = useIntegrationStore((s) => s.test);
  const loading = useIntegrationStore((s) => s.loading); const [message, setMessage] = useState("");
  return <section className="max-w-2xl space-y-4"><button onClick={onBack} className="text-sm text-ac-brand">Back to integrations</button><h1 className="text-xl font-semibold text-ac-ink">{integration.display_name}</h1><IntegrationStatusBadge status={integration.status} /><p className="text-sm text-ac-muted">{statusDescription(integration.status)}</p><ul className="list-disc pl-5 text-sm text-ac-muted">{integration.capabilities.map((capability) => <li key={capability.id}>{capability.display_name}: {capability.description}</li>)}</ul><div className="flex gap-2"><button disabled={loading} className="ac-btn px-3 py-1.5 text-xs" onClick={() => void (integration.enabled ? disable(integration.id) : enable(integration.id))}>{integration.enabled ? "Disable" : "Enable"}</button><button disabled={loading} className="rounded border border-ac-border px-3 py-1.5 text-xs" onClick={() => void test(integration.id).then((result) => setMessage(`Connection check: ${result.status}`))}>Test connection</button></div><p aria-live="polite" className="text-sm text-ac-muted">{message}</p></section>;
}

export function UserIntegrationOverview() {
  const available = useIntegrationStore((s) => s.availableDefinitions); const instances = useIntegrationStore((s) => s.instances); const loadAvailable = useIntegrationStore((s) => s.loadAvailable); const loadConfigured = useIntegrationStore((s) => s.loadConfigured); const handleEvent = useIntegrationStore((s) => s.handleEvent); const error = useIntegrationStore((s) => s.error);
  const [selected, setSelected] = useState<UserIntegrationDto | null>(null);
  useEffect(() => { void loadAvailable(); void loadConfigured(); let unlisten: (() => void) | undefined; void integrationService.subscribe(handleEvent).then((fn) => { unlisten = fn; }); return () => unlisten?.(); }, [handleEvent, loadAvailable, loadConfigured]);
  if (selected) return <UserIntegrationDetail integration={selected} onBack={() => setSelected(null)} />;
  const userIntegrations = Array.from(instances.values()).filter((view): view is { kind: "user"; data: UserIntegrationDto } => view.kind === "user").map((view) => view.data);
  return <section className="p-6"><h1 className="text-xl font-semibold text-ac-ink">Integrations</h1><p className="mt-1 text-sm text-ac-muted">Connect services you use with your workspace.</p>{error && <p role="alert" className="mt-3 text-sm text-ac-brand">{error}</p>}<div className="mt-5 grid gap-4 md:grid-cols-2">{userIntegrations.map((integration) => <UserIntegrationCard key={integration.id} integration={integration} onOpen={setSelected} />)}{available.filter((definition) => !userIntegrations.some((integration) => integration.definition_id === definition.id)).map((definition) => <article key={definition.id} className="rounded-lg border border-ac-border bg-ac-surface p-4"><h2 className="font-medium text-ac-ink">{definition.display_name}</h2><p className="text-sm text-ac-muted">{definition.description}</p><p className="mt-2 text-xs text-ac-muted">{definition.category}</p></article>)}</div></section>;
}

/** A deliberately safe user-managed setup surface: it never indicates or displays secrets. */
export function UserConnectForm({ onConnect }: { onConnect: (name: string) => void }) {
  const [name, setName] = useState("");
  return <form onSubmit={(event) => { event.preventDefault(); onConnect(name); setName(""); }} className="space-y-3"><label className="block text-sm text-ac-ink" htmlFor="integration-name">Connection name</label><input id="integration-name" value={name} onChange={(event) => setName(event.target.value)} className="w-full rounded border border-ac-border bg-ac-surface px-3 py-2 text-sm text-ac-ink" required /><button className="ac-btn px-3 py-1.5 text-xs" type="submit">Connect</button></form>;
}
