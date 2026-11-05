import { useDesktop } from '../../app/DesktopProvider';
import { useWorkspace } from './workspace-context';
import { text } from './text';
export function useConnections() {
  const { store, state, operationState } = useWorkspace(),
    { snapshot } = useDesktop();
  return {
    ...state,
    snapshot,
    operations: store.feedback,
    operationState,
    groupBusy: state.groupBusy || store.feedback.isBusy('group-layout'),
    store,
    refresh: store.refresh,
    folders: state.groups,
    available: state.transportReady && state.ready && !state.error,
    setNavigation: store.setNavigation,
    setGroupExpanded: store.setGroupExpanded,
    setGroupEditing: store.setGroupEditing,
    currentLayout: store.currentLayout,
    commitLayout: store.commitLayout,
    renameGroup: store.renameGroup,
    removeGroup: store.removeGroup,
    showNew: (groupId: string | null = null) => store.newConnection(groupId),
    createGroup: (surfaceId: string) =>
      store.createGroup(surfaceId, text(snapshot.appearance.locale, 'defaultGroupName')),
  };
}
