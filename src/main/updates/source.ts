export function updateSource(development: boolean, environment: NodeJS.ProcessEnv) {
  if (development && environment.VIGILSHELL_DEV_UPDATE_FEED) {
    const base = new URL(environment.VIGILSHELL_DEV_UPDATE_FEED);
    if (
      base.protocol !== 'http:' ||
      !['127.0.0.1', '[::1]'].includes(base.hostname) ||
      base.username ||
      base.password ||
      base.search ||
      base.hash
    )
      throw Error('invalid_dev_update_feed');
    const prefix = base.href.replace(/\/$/, '');
    return {
      downloadable: true,
      releases: `${prefix}/releases`,
      feed: (tag: string) => `${prefix}/${encodeURIComponent(tag)}/`,
    };
  }
  return {
    downloadable: !development,
    releases: 'https://api.github.com/repos/sparkwild/vigilshell/releases?per_page=100',
    feed: (tag: string) =>
      `https://github.com/sparkwild/vigilshell/releases/download/${encodeURIComponent(tag)}/`,
  };
}
