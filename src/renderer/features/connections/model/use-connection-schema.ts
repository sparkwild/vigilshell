import { hasC0Control } from '../../../../shared/text-validation';
import { useMemo } from 'react';

import { z } from 'zod';

import { useDesktop } from '../../../app/DesktopProvider';

import { validConnectionHost, terminalSettingsSchema, tunnelRuleSchema } from './form-validation';

export function useConnectionSchema() {
  const { t } = useDesktop();
  return useMemo(
    () =>
      z
        .object({
          notes: z
            .string()
            .refine((v) => utf8(v) <= 4096 && !v.includes('\0'), t('ws_field_notes')),
          terminal: terminalSettingsSchema,
          proxyId: z.uuid().nullable(),
          tunnels: z.array(tunnelRuleSchema).max(32),
          name: z
            .string()
            .trim()
            .refine((v) => utf8(v) <= 128 && !hasC0Control(v), t('ws_field_name')),
          host: z.string().refine(validConnectionHost, t('ws_field_host')),
          port: z
            .number({ error: t('ws_field_port') })
            .int(t('ws_field_port'))
            .min(1, t('ws_field_port'))
            .max(65535, t('ws_field_port')),
          username: z
            .string()
            .trim()
            .min(1, t('ws_field_required'))
            .refine((v) => utf8(v) <= 128 && !hasC0Control(v), t('ws_field_username')),
          auth: z.enum(['password', 'privateKey']),
          environment: z.enum(['none', 'development', 'test', 'staging', 'production']),
        })
        .transform((values) => ({
          ...values,
          name: values.name || values.host.trim(),
        }))
        .refine((values) => utf8(values.name) <= 128, {
          path: ['name'],
          message: t('ws_field_name'),
        }),
    [t],
  );
}

const utf8 = (value: string) => new TextEncoder().encode(value).length;
