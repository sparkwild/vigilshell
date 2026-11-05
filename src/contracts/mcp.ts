import type { AgentEvent, AgentReply, AgentSnapshot, AgentUiRequest } from './generated/runtime';
export type McpUiResult =
  | {
      ok: true;
      snapshot?: AgentSnapshot;
      reply?: Exclude<AgentReply, { kind: 'configuration' }>;
      directory?: string | null;
    }
  | { ok: false; code: string };
export type McpNavigation =
  | { view: 'activity'; taskId?: string; contextId?: string }
  | { view: 'policy'; connectionId: string | null };
export interface McpApi {
  request(request: AgentUiRequest): Promise<McpUiResult>;
  onChanged(listener: (event: AgentEvent) => void): () => void;
  onNavigate(listener: (target: McpNavigation) => void): () => void;
}
