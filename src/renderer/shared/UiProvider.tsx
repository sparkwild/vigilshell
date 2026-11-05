import type { ReactNode } from 'react';
import { I18nProvider } from 'react-aria-components';
import { useDesktop } from '../app/DesktopProvider';
import { TooltipProvider } from '../components/ui/tooltip';

export function UiProvider({ children }: { children: ReactNode }) {
  const { snapshot } = useDesktop();
  return (
    <I18nProvider locale={snapshot.appearance.locale}>
      <TooltipProvider delay={400}>{children}</TooltipProvider>
    </I18nProvider>
  );
}
