import type { ConnectionEnvironment } from '@contracts';
import { Badge } from '../../components/ui/badge';
import { TruncatedText } from '../../components/ui/tooltip';
import { useDesktop } from '../../app/DesktopProvider';
export const environments: ConnectionEnvironment[] = [
  'none',
  'development',
  'test',
  'staging',
  'production',
];
export const environmentKey = (environment: ConnectionEnvironment) =>
  environment === 'test' ? ('nav_environmentTest' as const) : (`nav_${environment}` as const);
export function EnvironmentBadge({
  environment,
  showNone = false,
  compact = false,
}: {
  environment: ConnectionEnvironment;
  showNone?: boolean;
  compact?: boolean;
}) {
  const { t } = useDesktop();
  return environment === 'none' && !showNone ? null : (
    <Badge
      compact={compact}
      className="min-w-0 max-w-[min(8rem,100%)] shrink"
      variant={environment === 'none' ? 'secondary' : `environment-${environment}`}
    >
      <TruncatedText className="min-w-0 truncate">{t(environmentKey(environment))}</TruncatedText>
    </Badge>
  );
}
