import { sha256 } from '@noble/hashes/sha2.js';
import { bytesToHex } from '@noble/hashes/utils.js';
import type {
  ExecutionPolicy,
  ExecutionRequest,
  ExecutionState,
  LocalDirectoryGrant,
  NativeServiceDefinition,
  PackageSourceDefinition,
  PackageTarget,
  RuntimeOperation,
  ScopedPath,
  ScriptReviewBinding,
} from './index';

const bytes = (value: string) => new TextEncoder().encode(value).length;
const control = /\p{Cc}/u;
const identity = (value: string) => /^[A-Za-z0-9_-]{1,128}$/.test(value);
const integer = (value: number, max = Number.MAX_SAFE_INTEGER, min = 0) =>
  Number.isSafeInteger(value) && value >= min && value <= max;
const hash = (value: string) => /^[\da-fA-F]{64}$/.test(value);
const text = (value: string, limit: number) =>
  /[^\p{White_Space}]/u.test(value) && bytes(value) <= limit && !value.includes('\0');
const argumentsValid = (values: string[]) =>
  values.length <= 128 &&
  values.every((value) => !value.includes('\0')) &&
  values.reduce((sum, value) => sum + bytes(value), 0) <= 64 * 1024;

// 与 Domain normalize_path 保持一致：任何 .. 分量均拒绝，不折叠到父目录。
function normalizedPath(path: string): string | null {
  if (path.length === 0 || bytes(path) > 4096 || control.test(path)) return null;
  const parts = path.split('/');
  if (parts.includes('..')) return null;
  return `/${parts.filter((part) => part !== '' && part !== '.').join('/')}`;
}
const absolutePath = (path: string) => path.startsWith('/') && normalizedPath(path) === path;
const beneath = (root: string, path: string) =>
  root === '/' || path === root || path.startsWith(`${root}/`);
function remotePath(target: ScopedPath): string | null {
  return target.scopeId === '__account__' && !target.path.startsWith('/')
    ? normalizedPath(target.path || '.')
    : null;
}
function grantsValid(grants: LocalDirectoryGrant[]): boolean {
  return (
    grants.length <= 128 &&
    new Set(grants.map((grant) => grant.id)).size === grants.length &&
    new Set(grants.map((grant) => grant.path)).size === grants.length &&
    grants.every(
      (grant) =>
        identity(grant.id) &&
        grant.id !== '__account__' &&
        grant.path !== '/' &&
        absolutePath(grant.path),
    )
  );
}
export function executionPolicyValid(policy: ExecutionPolicy): boolean {
  const limits = policy.limits;
  return (
    policy.version === 2 &&
    policy.protectedPaths.length <= 128 &&
    new Set(policy.protectedPaths.map((target) => target.path)).size ===
      policy.protectedPaths.length &&
    policy.protectedPaths.every((target) => absolutePath(target.path)) &&
    grantsValid(policy.uploadRoots) &&
    grantsValid(policy.downloadRoots) &&
    [
      limits.maxTaskSeconds,
      limits.terminalSeconds,
      limits.terminalIdleSeconds,
      limits.transferSeconds,
      limits.transferStallSeconds,
    ].every((value) => value == null || integer(value, 0xffff_ffff, 1)) &&
    [limits.concurrentTasks, limits.concurrentTransfers, limits.concurrentTerminals].every(
      (value) => integer(value, 16, 1),
    )
  );
}

function serviceName(name: string): boolean {
  const value = name.startsWith('user:') ? name.slice(5) : name;
  return (
    bytes(value) <= 256 &&
    /^[A-Za-z0-9._@][A-Za-z0-9._@-]*$/.test(value) &&
    name.endsWith('.service')
  );
}
const dropIn = (name: string) =>
  name.length <= 128 && /^[A-Za-z0-9][A-Za-z0-9._-]*\.conf$/.test(name);
function serviceDefinition(definition: NativeServiceDefinition, update: boolean): boolean {
  const { target, content, expectedHash } = definition;
  return (
    !target.name.startsWith('user:') &&
    target.name !== '.service' &&
    serviceName(target.name) &&
    update === (expectedHash != null) &&
    (expectedHash == null || hash(expectedHash)) &&
    (content.kind !== 'dropIn' || dropIn(content.name)) &&
    text(content.text, 256 * 1024)
  );
}
function packageValid(target: PackageTarget, remove: boolean): boolean {
  return target.kind === 'repository'
    ? target.name.length <= 256 &&
        /^[A-Za-z0-9][A-Za-z0-9.+_:-]*$/.test(target.name) &&
        (target.version == null ||
          (!remove && target.version.length <= 256 && /^[A-Za-z0-9.+~:_-]+$/.test(target.version)))
    : !remove &&
        absolutePath(target.path) &&
        /\.(deb|rpm)$/.test(target.path) &&
        hash(target.sha256);
}
const sourceWord = (value: string) =>
  value.length <= 128 && /^[A-Za-z0-9._-]+$/.test(value) && value !== '.' && value !== '..';
function packageSource(source: PackageSourceDefinition): boolean {
  return (
    (source.format !== 'apt' ||
      (sourceWord(source.suite) &&
        source.components.length > 0 &&
        source.components.length <= 16 &&
        source.components.every(sourceWord))) &&
    sourceWord(source.name) &&
    source.name.length <= 64 &&
    /^https?:\/\/[^/]+/.test(source.url) &&
    bytes(source.url) <= 2048 &&
    !/[\p{Cc}\p{White_Space}@#?\\$]/u.test(source.url) &&
    source.public_key.startsWith('-----BEGIN PGP PUBLIC KEY BLOCK-----') &&
    !source.public_key.includes('PRIVATE KEY') &&
    bytes(source.public_key) <= 128 * 1024
  );
}
function domainName(value: string, limit: number): boolean {
  return (
    value.length > 0 &&
    bytes(value) <= limit &&
    value
      .split('.')
      .every(
        (label) => label.length <= 63 && /^[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?$/.test(label),
      )
  );
}
function dnsAddress(value: string): boolean {
  if (/^(?:0|[1-9]\d{0,2})(?:\.(?:0|[1-9]\d{0,2})){3}$/.test(value)) {
    const parts = value.split('.').map(Number);
    const first = parts[0];
    return (
      first !== undefined &&
      parts.every((part) => part <= 255) &&
      value !== '0.0.0.0' &&
      !(first >= 224 && first <= 239)
    );
  }
  if (!value.includes(':') || !/^[\da-fA-F:.]+$/.test(value)) return false;
  // URL 规范化用于 IPv6 语法；IPv4 尾部必须另外遵守 Rust IpAddr 的十进制语法。
  if (value.includes('.')) {
    const suffix = value.slice(value.lastIndexOf(':') + 1);
    if (
      !/^(?:0|[1-9]\d{0,2})(?:\.(?:0|[1-9]\d{0,2})){3}$/.test(suffix) ||
      suffix.split('.').some((part) => Number(part) > 255)
    )
      return false;
  }
  try {
    const normalized = new URL(`http://[${value}]/`).hostname;
    return normalized !== '[::]' && !normalized.startsWith('[ff');
  } catch {
    return false;
  }
}
function operationTargetsValid(operation: RuntimeOperation): boolean {
  const targets: ScopedPath[] = [];
  if ('directory' in operation) targets.push(operation.directory);
  if ('target' in operation) targets.push(operation.target);
  if ('remote' in operation) targets.push(operation.remote);
  if (operation.kind === 'script' && operation.source.kind === 'remoteFile')
    targets.push(operation.source.target);
  if (operation.kind === 'files_copy' || operation.kind === 'files_move') {
    targets.push(operation.source);
    const source = remotePath(operation.source),
      target = remotePath(operation.target);
    if (
      source === null ||
      target === null ||
      source === target ||
      ((operation.kind === 'files_move' || operation.recursive) && beneath(source, target))
    )
      return false;
  }
  if (operation.kind === 'files_link') targets.push(operation.destination);
  return targets.every((target) => remotePath(target) !== null);
}

// 仅镜像 Rust 反序列化的业务参数不变量；权限判断和命令检查仍由 Domain 执行。
export function executionRequestValid(request: ExecutionRequest): boolean {
  if (
    request.requestKey.length === 0 ||
    bytes(request.requestKey) > 128 ||
    control.test(request.requestKey)
  )
    return false;
  if (request.timeoutSeconds != null && !integer(request.timeoutSeconds, 0xffff_ffff, 1))
    return false;
  const operation = request.operation;
  if (!operationTargetsValid(operation)) return false;
  switch (operation.kind) {
    case 'command':
      return text(operation.command, 64 * 1024);
    case 'script':
      return (
        argumentsValid(operation.arguments) &&
        (operation.source.kind === 'inline'
          ? text(operation.source.text, 256 * 1024)
          : operation.source.expected_hash == null || hash(operation.source.expected_hash))
      );
    case 'terminal':
      return integer(operation.columns, 500, 1) && integer(operation.rows, 300, 1);
    case 'transfers_upload':
    case 'transfers_download':
      return (
        identity(operation.local.scopeId) &&
        operation.local.scopeId !== '__account__' &&
        !operation.local.path.startsWith('/') &&
        normalizedPath(operation.local.path || '.') !== null
      );
    case 'files_list':
      return (
        integer(operation.limit, 500, 1) &&
        (operation.after == null ||
          (bytes(operation.after) <= 255 && !/[/\0]/.test(operation.after)))
      );
    case 'files_read':
      return integer(operation.offset) && integer(operation.length, 64 * 1024, 1);
    case 'files_write':
      return (
        bytes(operation.data) <= 128 * 1024 &&
        (operation.expected_hash == null || hash(operation.expected_hash))
      );
    case 'files_edit':
      return (
        hash(operation.expected_hash) &&
        operation.edits.length > 0 &&
        operation.edits.length <= 128 &&
        operation.edits.every((edit) => edit.before.length > 0) &&
        operation.edits.reduce((sum, edit) => sum + bytes(edit.before) + bytes(edit.after), 0) <=
          1024 * 1024
      );
    case 'files_set_attributes':
      return (
        (operation.mode != null || operation.modified_seconds != null) &&
        (operation.mode == null || integer(operation.mode, 0o777)) &&
        (operation.modified_seconds == null || integer(operation.modified_seconds))
      );
    case 'services_status':
    case 'services_definition':
      return serviceName(operation.name);
    case 'services_logs':
      return serviceName(operation.name) && integer(operation.lines, 10_000, 1);
    case 'services_control':
      return (
        serviceName(operation.name) && operation.action !== 'status' && operation.action !== 'logs'
      );
    case 'services_set_enabled':
      return (
        serviceName(operation.name) &&
        (operation.expected_version == null || hash(operation.expected_version))
      );
    case 'services_register':
    case 'services_update':
      return serviceDefinition(operation.definition, operation.kind === 'services_update');
    case 'services_unregister':
      return (
        serviceName(operation.name) &&
        hash(operation.expected_hash) &&
        (operation.drop_in == null || dropIn(operation.drop_in))
      );
    case 'packages_install':
    case 'packages_update':
    case 'packages_remove':
      return (
        operation.packages.length > 0 &&
        operation.packages.length <= 128 &&
        operation.packages.every((target) =>
          packageValid(target, operation.kind === 'packages_remove'),
        )
      );
    case 'package_sources_create':
      return packageSource(operation.definition);
    case 'package_sources_update': {
      const patch = operation.patch;
      return (
        hash(operation.source_id) &&
        hash(operation.expected_hash) &&
        [patch.url, patch.suite, patch.components, patch.public_key].some(
          (value) => value != null,
        ) &&
        packageSource({
          format: 'apt',
          name: 'validation',
          url: patch.url ?? 'https://validation.invalid',
          suite: patch.suite ?? 'stable',
          components: patch.components ?? ['main'],
          public_key: patch.public_key ?? '-----BEGIN PGP PUBLIC KEY BLOCK-----',
        })
      );
    }
    case 'package_sources_remove':
    case 'package_sources_manage':
      return hash(operation.source_id) && hash(operation.expected_hash);
    case 'system_settings_update':
      switch (operation.setting) {
        case 'hostname':
          return domainName(operation.value, 64);
        case 'timezone':
          return (
            operation.value.length <= 128 &&
            /^[A-Za-z0-9]/.test(operation.value) &&
            operation.value.split('/').every((part) => /^[A-Za-z0-9_+-]+$/.test(part))
          );
        case 'locale':
          return operation.value.length <= 64 && /^[A-Za-z][A-Za-z0-9_.@-]*$/.test(operation.value);
        default:
          return false;
      }
    case 'dns_update': {
      const { servers, searchDomains } = operation.configuration;
      return (
        servers.length > 0 &&
        servers.length <= 3 &&
        new Set(servers).size === servers.length &&
        servers.every(dnsAddress) &&
        searchDomains.length <= 6 &&
        searchDomains.reduce((sum, domain) => sum + bytes(domain), 0) <= 250 &&
        searchDomains.every((domain) => domainName(domain, 253)) &&
        (operation.expected_hash == null || hash(operation.expected_hash))
      );
    }
    default:
      return true;
  }
}

export function scriptReviewBindingValid(binding: ScriptReviewBinding): boolean {
  const snapshot = binding.snapshot;
  return (
    absolutePath(snapshot.path) &&
    absolutePath(snapshot.directory) &&
    text(snapshot.text, 256 * 1024) &&
    argumentsValid(snapshot.arguments) &&
    (snapshot.sourceVersion == null || hash(snapshot.sourceVersion)) &&
    snapshot.hash === bytesToHex(sha256(new TextEncoder().encode(snapshot.text)))
  );
}
export function executionStateValid(state: ExecutionState): boolean {
  const attempted = state.dispatch.kind === 'attempted';
  const permitted = state.review.kind === 'notRequired' || state.review.kind === 'approved';
  if (state.terminal && state.review.kind !== 'notRequired') return false;
  switch (state.phase) {
    case 'waitingReview':
      return !attempted && state.outcome == null && state.review.kind === 'waitingReview';
    case 'queued':
      return !attempted && state.outcome == null && permitted;
    case 'dispatching':
    case 'running':
    case 'stopping':
      return attempted && state.outcome == null && permitted;
    case 'settled':
      return (
        state.outcome != null &&
        (state.outcome.kind === 'notExecuted'
          ? !attempted && (state.review.kind !== 'rejected' || state.outcome.reason === 'denied')
          : attempted && permitted)
      );
  }
}
