import { translate } from './i18n/messages';
import type { ResolvedLocale } from '@contracts';
import { mcpIdentity } from './mcp-identity';

export function mcpHttpConfiguration(endpoint: string, token: string) {
  const { name } = mcpIdentity();
  return JSON.stringify(
    {
      mcpServers: {
        [name]: { type: 'http', url: endpoint, headers: { Authorization: `Bearer ${token}` } },
      },
    },
    null,
    2,
  );
}

export function mcpInstructions(locale: ResolvedLocale, endpoint: string, token: string) {
  return translate('agent_setup_instructions', locale, {
    configuration: mcpHttpConfiguration(endpoint, token),
  });
}
