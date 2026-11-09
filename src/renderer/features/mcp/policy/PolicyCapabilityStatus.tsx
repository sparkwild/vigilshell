import { useEffect, useState } from 'react';
import type { AgentReply, ConnectionPolicy } from '@contracts';
import { useDesktop } from '../../../app/DesktopProvider';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '../../../components/ui/dialog';
import { Button } from '../../../components/ui/button';
import { FieldDescription, FieldGroup } from '../../../components/ui/field';
import {
  Table,
  TableHeader,
  TableBody,
  Column,
  Row,
  Cell,
} from '../../../components/ui/data-table';
import { McpFeedback } from '../McpFeedback';
import { userErrorKey } from '../../../../shared/i18n/operation-errors';
import { capabilityLabels } from './capability-labels';

type Preview = Extract<AgentReply, { kind: 'policyCapabilities' }>;
export function PolicyCapabilityStatus({
  policy,
  valid,
  environmentKey,
}: {
  policy: ConnectionPolicy;
  valid: boolean;
  environmentKey: string;
}) {
  const { t } = useDesktop();
  const [open, setOpen] = useState(false);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!open || !valid) return;
    let live = true;
    setPreview(null);
    setError(null);
    const timer = setTimeout(() => {
      void window.vigilshell.mcp
        .request({
          action: 'previewPolicy',
          policy: {
            connectionId: policy.connectionId,
            connectionRevision: policy.connectionRevision,
            revision: policy.revision,
            verifyConnect: policy.verifyConnect,
            execution: policy.execution,
          },
        })
        .then((result) => {
          if (!live) return;
          if (result.ok && result.reply?.kind === 'policyCapabilities') setPreview(result.reply);
          else setError(result.ok ? 'invalid_response' : result.code);
        })
        .catch(() => {
          if (live) setError('runtime_unavailable');
        });
    }, 150);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [open, policy, valid, environmentKey]);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button variant="outline" />}>
        {t('policy_capability_title')}
      </DialogTrigger>
      <DialogContent size="wide" closeLabel={t('ws_close')}>
        <DialogHeader>
          <DialogTitle>{t('policy_capability_title')}</DialogTitle>
        </DialogHeader>
        <FieldGroup className="gap-3 py-3">
          <FieldDescription>{t('policy_capability_hint')}</FieldDescription>
          <McpFeedback code={error} />
          {!valid ? (
            <FieldDescription>{t('policy_capability_invalid')}</FieldDescription>
          ) : !preview && !error ? (
            <FieldDescription role="status">{t('ws_loading')}</FieldDescription>
          ) : null}
          {valid && preview ? (
            <>
              {!preview.environment_known ? (
                <FieldDescription>{t('policy_capability_unknown')}</FieldDescription>
              ) : null}
              <div className="max-h-96 overflow-auto">
                <Table
                  aria-label={t('policy_capability_title')}
                  selectionMode="none"
                  className="file-table w-full"
                >
                  <TableHeader>
                    <Column isRowHeader>{t('policy_capability_action')}</Column>
                    <Column>{t('policy_capability_grant')}</Column>
                    <Column>{t('policy_capability_environment')}</Column>
                    <Column>{t('policy_capability_effective')}</Column>
                  </TableHeader>
                  <TableBody items={preview.tools}>
                    {(item) => (
                      <Row id={item.tool}>
                        <Cell>{t(capabilityLabels[item.tool] ?? 'policy_capability_other')}</Cell>
                        <Cell>
                          {t(
                            item.configured
                              ? 'policy_capability_allowed'
                              : 'policy_capability_denied',
                          )}
                        </Cell>
                        <Cell>
                          {t(
                            !preview.environment_known
                              ? 'policy_capability_unchecked'
                              : item.supported
                                ? 'policy_capability_supported'
                                : 'policy_capability_unsupported',
                          )}
                        </Cell>
                        <Cell>
                          {t(
                            item.usable
                              ? 'policy_capability_ready'
                              : item.reason === 'session_not_ready'
                                ? 'policy_capability_unchecked'
                                : item.reason === 'policy_denied'
                                  ? 'policy_capability_denied'
                                  : item.reason === 'operation_unsupported'
                                    ? 'policy_capability_unsupported'
                                    : userErrorKey(item.reason),
                          )}
                        </Cell>
                      </Row>
                    )}
                  </TableBody>
                </Table>
              </div>
            </>
          ) : null}
        </FieldGroup>
      </DialogContent>
    </Dialog>
  );
}
