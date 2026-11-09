import { IconButton } from '../../components/ui/icon-button';
import { useEffect, useRef } from 'react';
import {
  TerminalIcon,
  HashIcon,
  ListFilterIcon,
  Maximize2Icon,
  Minimize2Icon,
  CircleFadingPlusIcon,
  SearchIcon,
  XIcon,
} from 'lucide-react';
import { Button } from '../../components/ui/button';
import { Badge } from '../../components/ui/badge';
import { InputGroup, InputGroupInput, InputGroupAddon } from '../../components/ui/input-group';
import { TabsList, TabsTrigger, TabsIndicator } from '../../components/ui/tabs';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuSub,
  DropdownMenuSubTrigger,
  DropdownMenuSubContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuItem,
} from '../../components/ui/dropdown-menu';
import { Tooltip, TooltipTrigger, TooltipContent } from '../../components/ui/tooltip';
import { useConnections } from './ConnectionProvider';
import { OperationIndicator } from './OperationFeedback';
import { hasFilters, resetFilterSort, type NavigationOptions } from './navigation';
import { text, type ConnectionText } from './text';

export function ConnectionNavigationToolbar({ surfaceId }: { surfaceId: string }) {
  const searchInput = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const focus = (e: Event) => {
      const input = searchInput.current;
      if (
        (e as CustomEvent).detail === 'overview' &&
        input?.checkVisibility() &&
        !input.closest('[inert]')
      )
        input.focus();
    };
    window.addEventListener('vigilshell:find', focus);
    return () => window.removeEventListener('vigilshell:find', focus);
  }, []);
  const state = useConnections();
  const options = state.navigation;
  const t = (key: ConnectionText) => text(state.snapshot.appearance.locale, key);
  const groupIds = state.folders.map((folder) => `f:${folder.id}`);
  if (state.connections.some((connection) => !connection.groupId)) groupIds.push('u:root');
  const allGroupsExpanded =
    groupIds.length > 0 && groupIds.every((id) => state.groupExpanded.has(id));
  const toggleGroupsLabel = t(allGroupsExpanded ? 'collapseAllGroups' : 'expandAllGroups');
  function choice<K extends 'status' | 'sortBy' | 'direction'>(
    key: K,
    label: ConnectionText,
    items: { value: NavigationOptions[K]; label: string }[],
  ) {
    return (
      <DropdownMenuSub>
        <DropdownMenuSubTrigger>{t(label)}</DropdownMenuSubTrigger>
        <DropdownMenuSubContent aria-label={t(label)}>
          <DropdownMenuGroup>
            <DropdownMenuRadioGroup
              value={options[key]}
              onValueChange={(value) => {
                const item = items.find((item) => item.value === value);
                if (item)
                  state.setNavigation((previous) => ({
                    ...previous,
                    [key]: item.value,
                  }));
              }}
            >
              {items.map((item) => (
                <DropdownMenuRadioItem key={item.value} value={item.value} closeOnClick={false}>
                  {item.label}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuGroup>
        </DropdownMenuSubContent>
      </DropdownMenuSub>
    );
  }
  return (
    <>
      <div className="connection-navigation-toolbar">
        <TabsList
          variant="segmented"
          adaptiveLabels
          activateOnFocus
          aria-label={t('viewMode')}
          className="min-w-0"
        >
          <TabsIndicator />
          <TabsTrigger
            value="connections"
            aria-label={t('connections')}
            label={t('connections')}
            className="min-w-0"
          >
            <TerminalIcon />
          </TabsTrigger>
          <TabsTrigger
            value="groups"
            aria-label={t('groups')}
            label={t('groups')}
            className="min-w-0"
          >
            <HashIcon />
          </TabsTrigger>
        </TabsList>
        <div className="flex shrink-0 items-center gap-1">
          {options.view === 'groups' ? (
            <IconButton
              size="icon-xs"
              variant="titlebar"
              disabled={
                !groupIds.length || !state.transportReady || !state.groupsReady || state.groupBusy
              }
              onClick={() =>
                state.setGroupExpanded((previous) =>
                  groupIds.every((id) => previous.has(id)) ? new Set() : new Set(groupIds),
                )
              }
              label={toggleGroupsLabel}
              tooltip={<>{toggleGroupsLabel}</>}
            >
              {allGroupsExpanded ? <Minimize2Icon /> : <Maximize2Icon />}
            </IconButton>
          ) : null}
          {options.view === 'connections' ? (
            <DropdownMenu modal={false}>
              <Tooltip>
                <TooltipTrigger
                  render={
                    <DropdownMenuTrigger
                      render={
                        <Button
                          size="icon-xs"
                          variant="titlebar"
                          aria-label={t('filterSort')}
                          className="relative"
                        />
                      }
                    />
                  }
                >
                  <ListFilterIcon />
                  {hasFilters(options) ? (
                    <Badge className="connection-filter-indicator" aria-label={t('filtersActive')}>
                      {Number(options.status !== 'all')}
                    </Badge>
                  ) : null}
                </TooltipTrigger>
                <TooltipContent>{t('filterSort')}</TooltipContent>
              </Tooltip>
              <DropdownMenuContent
                align="end"
                sideOffset={8}
                aria-label={t('filterSort')}
                className="w-max min-w-45 max-w-(--available-width)"
              >
                <DropdownMenuGroup>
                  <DropdownMenuLabel>{t('filters')}</DropdownMenuLabel>
                  {choice('status', 'connectionState', [
                    { value: 'connected', label: t('connected') },
                    { value: 'disconnected', label: t('disconnected') },
                  ])}
                </DropdownMenuGroup>
                <DropdownMenuSeparator />
                <DropdownMenuGroup>
                  <DropdownMenuLabel>{t('sorting')}</DropdownMenuLabel>
                  {choice('direction', 'sortRule', [
                    { value: 'asc', label: t('ascending') },
                    { value: 'desc', label: t('descending') },
                  ])}
                  {choice('sortBy', 'sortMethod', [
                    { value: 'createdAt', label: t('createdAt') },
                    { value: 'name', label: t('connectionName') },
                  ])}
                </DropdownMenuGroup>
                <DropdownMenuSeparator />
                <DropdownMenuGroup>
                  <DropdownMenuItem
                    variant="destructive-subtle"
                    disabled={
                      !hasFilters(options) &&
                      options.sortBy === 'createdAt' &&
                      options.direction === 'asc'
                    }
                    closeOnClick={false}
                    onClick={() => state.setNavigation(resetFilterSort)}
                  >
                    {t('resetFilterSort')}
                  </DropdownMenuItem>
                </DropdownMenuGroup>
              </DropdownMenuContent>
            </DropdownMenu>
          ) : null}
          <OperationIndicator
            owner={'navigation:' + options.view}
            className="size-6"
            icon={
              <IconButton
                size="icon-xs"
                variant="titlebar"
                disabled={options.view === 'groups' && (!state.available || state.groupBusy)}
                onClick={() =>
                  options.view === 'connections'
                    ? state.showNew()
                    : void state.createGroup(surfaceId).catch(() => undefined)
                }
                label={t(options.view === 'connections' ? 'new' : 'newFolder')}
                tooltip={<>{t(options.view === 'connections' ? 'new' : 'newFolder')}</>}
              >
                <CircleFadingPlusIcon />
              </IconButton>
            }
          />
        </div>
      </div>
      {options.view === 'connections' ? (
        <InputGroup variant="sidebar">
          <InputGroupInput
            ref={searchInput}
            aria-label={t('search')}
            placeholder={t('searchPlaceholder')}
            value={options.search}
            onChange={(event) =>
              state.setNavigation((previous) => ({
                ...previous,
                search: event.target.value,
              }))
            }
          />
          <InputGroupAddon>
            <SearchIcon aria-hidden="true" />
          </InputGroupAddon>
          {options.search ? (
            <InputGroupAddon align="inline-end">
              <Button
                size="icon-xs"
                variant="titlebar"
                aria-label={t('clearSearch')}
                onClick={() => {
                  state.setNavigation((previous) => ({
                    ...previous,
                    search: '',
                  }));
                  searchInput.current?.focus();
                }}
              >
                <XIcon />
              </Button>
            </InputGroupAddon>
          ) : null}
        </InputGroup>
      ) : null}
    </>
  );
}
