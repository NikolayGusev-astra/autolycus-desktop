import type { GatewayEvent } from './types';

type JsonRpcRequest = {
  id: number;
  method: string;
  params?: Record<string, unknown>;
};

type JsonRpcError = { code: number; message: string };

type JsonRpcResponse = {
  id: number;
  result?: unknown;
  error?: JsonRpcError;
};

type EventHandler = (event: GatewayEvent) => void;
type RequestCallback = { resolve: (v: unknown) => void; reject: (e: Error) => void };

export class GatewayClient {
  private ws: WebSocket | null = null;
  private requestId = 0;
  private pending = new Map<number, RequestCallback>();
  private eventHandlers = new Set<EventHandler>();
  private reconnectTimer: number | null = null;
  private url: string;

  constructor(port: number) {
    this.url = `ws://127.0.0.1:${port}`;
  }

  async connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(this.url);
      this.ws.onopen = () => resolve();
      this.ws.onerror = () => reject(new Error('WebSocket connection failed'));
      this.ws.onmessage = (e) => this.handleMessage(JSON.parse(e.data));
      this.ws.onclose = () => this.scheduleReconnect();
    });
  }

  private handleMessage(data: JsonRpcResponse | GatewayEvent): void {
    if ('id' in data && typeof data.id === 'number') {
      const pending = this.pending.get(data.id);
      if (pending) {
        this.pending.delete(data.id);
        const err = data.error as JsonRpcError;
        if (err) pending.reject(new Error(err.message));
        else pending.resolve(data.result);
      }
    } else {
      this.eventHandlers.forEach((h) => h(data as GatewayEvent));
    }
  }

  async call(method: string, params?: Record<string, unknown>): Promise<unknown> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      throw new Error('WebSocket not connected');
    }
    const id = ++this.requestId;
    const req: JsonRpcRequest = { id, method, params };
    this.ws.send(JSON.stringify(req));
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id);
          reject(new Error(`Request timeout: ${method}`));
        }
      }, 30000);
    });
  }

  onEvent(handler: EventHandler): () => void {
    this.eventHandlers.add(handler);
    return () => this.eventHandlers.delete(handler);
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer) return;
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = null;
      this.connect().catch(() => this.scheduleReconnect());
    }, 2000);
  }

  disconnect(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (this.ws) {
      this.ws.onclose = null;
      this.ws.close();
      this.ws = null;
    }
    this.pending.forEach((cb) => cb.reject(new Error('Disconnected')));
    this.pending.clear();
  }

  get isConnected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }
}
