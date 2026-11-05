// Form validation and normalization only; wire validation comes from Rust Schema.
import { z } from 'zod';
const id = z.uuid();
const utf8Length = (value: string) => new TextEncoder().encode(value).length;
export function validConnectionHost(value: string): boolean {
  let host = value.trim();
  if (host.startsWith('[') && host.endsWith(']')) {
    host = host.slice(1, -1);
    if (!z.ipv6().safeParse(host).success) return false;
  }
  return (
    host.length > 0 &&
    utf8Length(host) <= 255 &&
    !/[\s\p{Cc}/\\@?#[\]"']/u.test(host) &&
    (!host.includes(':') || z.ipv6().safeParse(host).success)
  );
}
export const terminalSettingsSchema = z
  .object({
    encoding: z.enum(['utf-8', 'gb18030', 'gbk', 'big5']),
    backspace: z.enum(['del', 'bs']),
    delete: z.enum(['vt220', 'del']),
  })
  .strict();
export const tunnelRuleSchema = z
  .object({
    id,
    name: z
      .string()
      .trim()
      .min(1)
      .refine((v) => utf8Length(v) <= 128 && !/\p{Cc}/u.test(v)),
    kind: z.enum(['local', 'remote', 'socks5']),
    bindHost: z
      .string()
      .refine((v) => z.ipv4().safeParse(v).success || z.ipv6().safeParse(v).success),
    listenPort: z.number().int().min(1).max(65535),
    targetHost: z
      .string()
      .refine((v) => !v.includes('[') && !v.includes(']') && validConnectionHost(v))
      .nullable(),
    targetPort: z.number().int().min(1).max(65535).nullable(),
    enabled: z.boolean(),
  })
  .strict()
  .refine((v) =>
    v.kind === 'socks5'
      ? v.targetHost === null && v.targetPort === null
      : v.targetHost !== null && v.targetPort !== null,
  );
export const proxyConfigSchema = z
  .object({
    name: z
      .string()
      .trim()
      .min(1)
      .refine((v) => utf8Length(v) <= 128 && !/\p{Cc}/u.test(v)),
    kind: z.enum(['socks5', 'http']),
    host: z
      .string()
      .trim()
      .refine((v) => !v.includes('[') && !v.includes(']') && validConnectionHost(v)),
    port: z.number().int().min(1).max(65535),
    username: z.string().refine((v) => utf8Length(v) <= 255 && !/\p{Cc}/u.test(v)),
  })
  .strict()
  .refine((v) => v.kind !== 'http' || !v.username.includes(':'), {
    path: ['username'],
  });
export type ConnectionFormValues = Required<import('@contracts').ConnectionConfig>;
export function connectionFormValues(
  config: import('@contracts').ConnectionConfig,
): ConnectionFormValues {
  return {
    ...config,
    notes: config.notes ?? '',
    terminal: config.terminal ?? { encoding: 'utf-8', backspace: 'del', delete: 'vt220' },
    proxyId: config.proxyId ?? null,
    tunnels: config.tunnels ?? [],
    environment: config.environment ?? 'none',
  };
}

export type EditorSection = 'connection' | 'terminal' | 'proxy' | 'tunnels';
