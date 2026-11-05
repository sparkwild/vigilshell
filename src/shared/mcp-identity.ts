export function mcpIdentity() {
  const development =
    import.meta.env.VIGILSHELL_DEVELOPMENT_BUILD === '1' ||
    import.meta.env.VIGILSHELL_DEV_PACKAGE_BUILD === '1';
  return {
    name: development ? 'vigilshell-dev' : 'vigilshell',
    title: development ? 'VigilShell Dev' : 'VigilShell',
  };
}
