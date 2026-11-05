import { useRef, useState } from 'react';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from '../../components/ui/dropdown-menu';
import type { useMcp } from './model';
import { ChevronDownIcon, SparklesIcon } from 'lucide-react';
import type { AgentClient } from '@contracts';
import deepseek from './assets/deepseek.svg?url';
import codex from './assets/codex.svg?url';
import cursor from './assets/cursor.svg?url';
import claudeCode from './assets/claude.svg?url';
import mcp from './assets/mcp.svg?url';
export const clientNames = {
  codex: 'Codex',
  cursor: 'Cursor',
  claudeCode: 'Claude Code',
  dsh: 'DeepSeek Harness',
} satisfies Record<Exclude<AgentClient, 'generic'>, string>;
type LogoName = AgentClient | 'mcp';
const logos: Partial<Record<LogoName, string>> = { codex, cursor, claudeCode, dsh: deepseek, mcp };
export function AgentLogo({
  client,
  size = 'default',
}: {
  client?: LogoName | null;
  size?: 'default' | 'sm';
}) {
  return client && logos[client] ? (
    <span
      className="mcp-client-logo"
      data-size={size}
      style={{ maskImage: `url(${logos[client]})` }}
      aria-hidden="true"
    />
  ) : (
    <SparklesIcon className="mcp-client-symbol" data-size={size} aria-hidden="true" />
  );
}

export function McpClientPicker({
  model,
  disabled,
  onSelect,
}: {
  model: ReturnType<typeof useMcp>;
  disabled: boolean;
  onSelect(client: AgentClient): void;
}) {
  const { t } = useDesktop();
  const [status, setStatus] = useState<'idle' | 'loading' | 'ready' | 'failed'>('idle');
  const scanning = useRef(false);
  const choosing = useRef(false);
  async function scan() {
    if (scanning.current) return;
    scanning.current = true;
    setStatus('loading');
    try {
      setStatus((await model.issue({ action: 'detectClients' })) ? 'ready' : 'failed');
    } catch {
      setStatus('failed');
    } finally {
      scanning.current = false;
    }
  }
  const clients = model.snapshot!.clients;
  const installed =
    status === 'ready' ? clients.filter((c) => c.installed && c.client !== 'generic') : [];
  const incomplete = status === 'failed' || clients.some((c) => c.unavailable);
  function select(client: AgentClient) {
    choosing.current = true;
    onSelect(client);
  }
  return (
    <DropdownMenu
      onOpenChange={(open) => {
        if (open) {
          choosing.current = false;
          void scan();
        }
      }}
    >
      <DropdownMenuTrigger render={<Button size="sm" disabled={disabled} />}>
        {t('agent_add_connection')}
        <ChevronDownIcon data-icon="inline-end" />
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="end"
        className="min-w-64"
        finalFocus={() => (choosing.current ? false : true)}
      >
        <DropdownMenuGroup>
          <DropdownMenuLabel>{t('agent_local_clients')}</DropdownMenuLabel>
          {installed.map((item) => (
            <DropdownMenuItem
              key={item.client}
              disabled={disabled}
              onClick={() => select(item.client)}
            >
              <AgentLogo client={item.client} />
              {clientNames[item.client as keyof typeof clientNames]}
            </DropdownMenuItem>
          ))}
          {status === 'loading' || status === 'idle' || incomplete || !installed.length ? (
            <DropdownMenuItem disabled>
              {t(
                status === 'loading' || status === 'idle'
                  ? 'agent_scan_loading'
                  : incomplete
                    ? 'agent_detection_incomplete'
                    : 'agent_no_clients',
              )}
            </DropdownMenuItem>
          ) : null}
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuItem disabled={disabled} onClick={() => select('generic')}>
            <AgentLogo client="generic" />
            {t('agent_generic_name')}
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
