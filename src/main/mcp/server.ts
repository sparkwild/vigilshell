import { createServer, type Server } from 'node:http';
import { createHash } from 'node:crypto';
import { McpServer, createMcpHandler, fromJsonSchema } from '@modelcontextprotocol/server';
import { toNodeHandler } from '@modelcontextprotocol/node';
import schema from '../../contracts/generated/runtime.schema.json';
import type { AccessEntry, AgentCall, AgentReply } from '@contracts';
import { mcpIdentity } from '../../shared/mcp-identity';

export interface McpExecution {
  unavailable?(): void;
  observed(
    entryId: string,
    generation: number,
    port: number,
    endpointRevision: number,
  ): Promise<void>;
  authenticate(digest: string): Promise<AccessEntry & { endpointRevision: number }>;
  rejected(digest: string, tool: string, code: string): Promise<void>;
  invoke(digest: string, call: AgentCall): Promise<AgentReply>;
}
import { descriptions, readOnlyTools } from './tool-descriptions';

type JsonSchema = { [key: string]: unknown };
/** 只携带该工具引用的定义，避免把私有管理合同或整个桌面 Schema 发布出去。 */
function toolSchemas() {
  const definitions = schema.definitions as Record<string, JsonSchema>;
  const variants = definitions.AgentCall?.oneOf as JsonSchema[];
  return variants.map((variant) => {
    const properties = { ...(variant.properties as Record<string, JsonSchema>) };
    const action = properties.tool?.const as AgentCall['tool'];
    delete properties.tool;
    const selected: Record<string, JsonSchema> = {};
    function visit(value: unknown) {
      if (!value || typeof value !== 'object') return;
      if ('$ref' in value && typeof value.$ref === 'string') {
        const name = value.$ref.split('/').at(-1)!;
        if (!selected[name] && definitions[name]) {
          selected[name] = definitions[name];
          visit(selected[name]);
        }
      }
      for (const child of Object.values(value)) visit(child);
    }
    const branches = variant.oneOf as JsonSchema[] | undefined;
    const oneOf = branches?.map((branch) => {
      const inner = { ...(branch.properties as Record<string, JsonSchema>) };
      delete inner.tool;
      return {
        ...branch,
        properties: inner,
        additionalProperties: false,
        required: (branch.required as string[]).filter((key) => key !== 'tool'),
      };
    });
    visit(properties);
    visit(oneOf);
    return {
      action,
      schema: {
        ...variant,
        properties,
        ...(oneOf ? { oneOf } : { additionalProperties: false }),
        required: (variant.required as string[]).filter((key) => key !== 'tool'),
        definitions: selected,
      },
    };
  });
}

export class LocalMcpServer {
  private server: Server | null = null;
  private portValue = 0;
  private active = 0;
  private readonly handler;
  private readonly validators = new Map<string, (value: unknown) => Promise<boolean>>();
  constructor(private readonly execution: McpExecution) {
    const schemas = toolSchemas().map(({ action, schema }) => ({
      action,
      input: fromJsonSchema<Record<string, unknown>>(schema),
    }));
    for (const { action, input } of schemas)
      this.validators.set(
        action,
        async (value) => !(await input['~standard'].validate(value)).issues,
      );
    this.handler = createMcpHandler(
      (context) => {
        const digest = context.authInfo?.token;
        if (!digest) throw new Error('access_denied');
        const server = new McpServer({ ...mcpIdentity(), version: '0.1.0' });
        for (const { action, input } of schemas) {
          server.registerTool(
            action,
            {
              description: descriptions[action],
              inputSchema: input,
              annotations: {
                readOnlyHint: readOnlyTools.has(action),
                destructiveHint: !readOnlyTools.has(action),
                openWorldHint: true,
              },
            },
            async (args) => {
              const result = await this.execution.invoke(digest, {
                ...args,
                tool: action,
              } as AgentCall);
              if (context.authInfo?.clientId)
                await this.execution
                  .observed(
                    context.authInfo.clientId,
                    Number(context.authInfo.extra?.generation),
                    Number(context.authInfo.extra?.port),
                    Number(context.authInfo.extra?.endpointRevision),
                  )
                  .catch(() => undefined);
              const isError =
                result.kind === 'error' ||
                (result.kind === 'task' && result.task.phase === 'denied');
              return {
                isError,
                content: [{ type: 'text' as const, text: JSON.stringify(result) }],
                structuredContent: result,
              };
            },
          );
        }
        return server;
      },
      { legacy: 'stateless', maxRequestBodySize: 2 * 1024 * 1024 },
    );
  }
  get port() {
    return this.portValue;
  }
  async start(port: number): Promise<number> {
    if (this.server) return this.portValue;
    const handle = toNodeHandler(
      {
        fetch: async (request, options) => {
          let protocolMethod = '';
          if (request.method === 'POST') {
            const body = (await request
              .clone()
              .json()
              .catch(() => null)) as {
              method?: unknown;
              params?: { name?: unknown; arguments?: unknown };
            } | null;
            protocolMethod = typeof body?.method === 'string' ? body.method : '';
            if (body?.method === 'tools/call') {
              const name = typeof body.params?.name === 'string' ? body.params.name : 'unknown';
              const valid = this.validators.get(name);
              const code = !valid
                ? 'unsupported_tool'
                : !(await valid(body.params?.arguments ?? {}))
                  ? 'invalid_tool_arguments'
                  : null;
              if (code && options?.authInfo?.token)
                await this.execution.rejected(options.authInfo.token, name.slice(0, 128), code);
            }
          }
          const response = await this.handler.fetch(request, options);
          if (request.method === 'POST' && response.ok && options?.authInfo) {
            if (['initialize', 'tools/list', 'ping'].includes(protocolMethod)) {
              const data = await response.clone().text();
              const envelopes =
                data.startsWith('data:') ||
                response.headers.get('content-type')?.includes('text/event-stream')
                  ? data
                      .split('\n')
                      .filter((l) => l.startsWith('data:'))
                      .map((l) => l.slice(5))
                  : [data];
              if (
                envelopes.some((value) => {
                  try {
                    const v = JSON.parse(value);
                    return v.jsonrpc === '2.0' && 'result' in v && !v.error;
                  } catch {
                    return false;
                  }
                })
              ) {
                await this.execution
                  .observed(
                    options.authInfo.clientId,
                    Number(options.authInfo.extra?.generation),
                    Number(options.authInfo.extra?.port),
                    Number(options.authInfo.extra?.endpointRevision),
                  )
                  .catch(() => undefined);
              }
            }
          }
          return response;
        },
      },
      { maxRequestBodySize: 2 * 1024 * 1024 },
    );
    const server = createServer((request, response) => {
      const reject = (status: number) => {
        response.writeHead(status, { 'Content-Type': 'application/json' });
        response.end(
          JSON.stringify({ error: status === 401 ? 'access_denied' : 'request_rejected' }),
        );
      };
      if (request.url !== '/mcp') {
        reject(404);
        return;
      }
      if (
        request.headers.host !== `127.0.0.1:${this.portValue}` ||
        request.headers.origin !== undefined
      ) {
        reject(403);
        return;
      }
      const authorization = request.headers.authorization;
      if (!authorization || !/^Bearer [A-Za-z0-9_-]{43}$/.test(authorization)) {
        reject(401);
        return;
      }
      if (Number(request.headers['content-length']) > 2 * 1024 * 1024) {
        // 提前回应已知超限正文并丢弃流，避免 SDK 中断读取时重置客户端连接。
        request.resume();
        reject(413);
        return;
      }
      if (this.active >= 32) {
        reject(429);
        return;
      }
      this.active++;
      const digest = createHash('sha256').update(authorization.slice(7)).digest('hex');
      void (async () => {
        try {
          const entry = await this.execution.authenticate(digest);
          Object.assign(request, {
            auth: {
              token: digest,
              clientId: entry.id,
              scopes: [],
              extra: {
                generation: entry.generation,
                port: this.portValue,
                endpointRevision: entry.endpointRevision,
              },
            },
          });
          await handle(request, response);
        } catch {
          if (!response.headersSent) reject(401);
          else response.destroy();
        } finally {
          this.active--;
        }
      })();
    });
    server.headersTimeout = 10_000;
    server.requestTimeout = 30_000;
    server.timeout = 30_000;
    await new Promise<void>((resolve, reject) => {
      server.once('error', reject);
      server.listen(port, '127.0.0.1', () => {
        server.removeListener('error', reject);
        resolve();
      });
    });
    const address = server.address();
    if (!address || typeof address === 'string') {
      server.close();
      throw new Error('listener_failed');
    }
    this.server = server;
    this.portValue = address.port;
    const lost = () => {
      if (this.server !== server) return;
      this.server = null;
      this.portValue = 0;
      server.closeAllConnections();
      if (server.listening) server.close();
      this.execution.unavailable?.();
    };
    server.on('error', lost);
    server.once('close', lost);
    return this.portValue;
  }
  async stop() {
    const server = this.server;
    this.server = null;
    this.portValue = 0;
    if (server) {
      server.closeAllConnections();
      await new Promise<void>((resolve) => server.close(() => resolve()));
    }
  }
  async dispose() {
    await this.stop();
    await this.handler.close();
  }
}
