#!/bin/bash
set -euo pipefail
[[ "${GITHUB_ACTIONS:-}" == true && "${RUNNER_ENVIRONMENT:-}" == github-hosted && "${RUNNER_OS:-}" == macOS ]]
: "${RUNNER_TEMP:?}"
CI_SIGNING_ROOT="$RUNNER_TEMP/vigilshell-ci-signing"
CI_KEYCHAIN="$CI_SIGNING_ROOT/ci.keychain-db"
case "${1:-}" in
  setup|setup-release)
    if [[ "$1" == setup-release ]]; then
      [[ "${GITHUB_REPOSITORY:-}" == sparkwild/vigilshell ]]
      if [[ "${GITHUB_EVENT_NAME:-}" == workflow_dispatch ]]; then
        [[ "${GITHUB_REF:-}" == refs/heads/main || "${GITHUB_REF:-}" == refs/heads/develop ]]
      else
        [[ "${GITHUB_EVENT_NAME:-}" == push && "${GITHUB_REF:-}" =~ ^refs/tags/v[0-9]+\.[0-9]+\.[0-9]+-beta\.[1-9][0-9]*$ ]]
      fi
      : "${CERTIFICATE_BASE64:?Configure MACOS_SIGNING_P12_BASE64 in macos-release}"
      : "${CERTIFICATE_PASSWORD:?Configure MACOS_SIGNING_P12_PASSWORD in macos-release}"
      : "${VIGILSHELL_RELEASE_SIGNING_IDENTITY:?Configure MACOS_SIGNING_IDENTITY in the selected storage}"
      node scripts/release-policy.mjs --source
    fi
    mkdir -m 700 "$CI_SIGNING_ROOT"
    umask 077
    # Exported keys are never needed after import, including on setup failure.
    trap 'rm -f "$CI_SIGNING_ROOT/key.pem" "$CI_SIGNING_ROOT/identity.p12"' EXIT
    security list-keychains -d user > "$CI_SIGNING_ROOT/keychains-before.txt"
    CI_PASSWORD="$(openssl rand -hex 32)"
    echo "::add-mask::$CI_PASSWORD"
    if [[ "$1" == setup ]]; then
      cat > "$CI_SIGNING_ROOT/certificate.conf" <<'CONF'
[req]
distinguished_name = subject
x509_extensions = signing
prompt = no
[subject]
CN = VigilShell CI
[signing]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
CONF
      openssl req -x509 -newkey rsa:2048 -nodes -days 2 \
        -config "$CI_SIGNING_ROOT/certificate.conf" \
        -keyout "$CI_SIGNING_ROOT/key.pem" -out "$CI_SIGNING_ROOT/certificate.pem"
      openssl pkcs12 -export -inkey "$CI_SIGNING_ROOT/key.pem" \
        -in "$CI_SIGNING_ROOT/certificate.pem" -out "$CI_SIGNING_ROOT/identity.p12" \
        -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1 -passout "pass:$CI_PASSWORD"
      IMPORT_PASSWORD="$CI_PASSWORD"
    else
      printf '%s' "$CERTIFICATE_BASE64" | base64 --decode > "$CI_SIGNING_ROOT/identity.p12"
      IMPORT_PASSWORD="$CERTIFICATE_PASSWORD"
    fi
    security create-keychain -p "$CI_PASSWORD" "$CI_KEYCHAIN"
    security set-keychain-settings -lut 21600 "$CI_KEYCHAIN"
    security unlock-keychain -p "$CI_PASSWORD" "$CI_KEYCHAIN"
    security import "$CI_SIGNING_ROOT/identity.p12" -k "$CI_KEYCHAIN" -P "$IMPORT_PASSWORD" -T /usr/bin/codesign
    rm -f "$CI_SIGNING_ROOT/key.pem" "$CI_SIGNING_ROOT/identity.p12"
    if [[ "$1" == setup-release ]]; then
      security export -k "$CI_KEYCHAIN" -t certs -f pemseq -o "$CI_SIGNING_ROOT/certificates.pem"
      node --input-type=module - "$CI_SIGNING_ROOT" <<'JS'
import assert from 'node:assert/strict';
import { X509Certificate } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { normalizeFingerprint } from './scripts/signing/config.mjs';
const root = process.argv[2];
const expected = normalizeFingerprint(process.env.VIGILSHELL_RELEASE_SIGNING_IDENTITY);
const certificates = readFileSync(join(root, 'certificates.pem'), 'utf8').match(/-----BEGIN CERTIFICATE-----[\s\S]*?-----END CERTIFICATE-----/g) ?? [];
const selected = certificates.filter((pem) => normalizeFingerprint(new X509Certificate(pem).fingerprint) === expected);
assert.equal(selected.length, 1, 'Fixed release certificate must match MACOS_SIGNING_IDENTITY');
writeFileSync(join(root, 'certificate.pem'), selected[0], { mode: 0o600 });
JS
    fi
    echo "Authorizing CI signing key"
    security set-key-partition-list -S apple-tool:,apple: -s -k "$CI_PASSWORD" "$CI_KEYCHAIN" >/dev/null
    echo "Trusting CI signing certificate"
    sudo -n security add-trusted-cert -d -r trustRoot -p codeSign -k "$CI_KEYCHAIN" "$CI_SIGNING_ROOT/certificate.pem"
    CI_IDENTITY="$(openssl x509 -in "$CI_SIGNING_ROOT/certificate.pem" -noout -fingerprint -sha1 | cut -d= -f2 | tr -d ':')"
    security list-keychains -d user -s "$CI_KEYCHAIN" "$HOME/Library/Keychains/login.keychain-db"
    echo "VIGILSHELL_RELEASE_SIGNING_IDENTITY=$CI_IDENTITY" >> "$GITHUB_ENV"
    echo "VIGILSHELL_RELEASE_SIGNING_KEYCHAIN=$CI_KEYCHAIN" >> "$GITHUB_ENV"
    export VIGILSHELL_RELEASE_SIGNING_IDENTITY="$CI_IDENTITY"
    export VIGILSHELL_RELEASE_SIGNING_KEYCHAIN="$CI_KEYCHAIN"
    npm run signing:check -- --profile release
    ;;
  cleanup)
    node "$(dirname "$0")/ci-macos-cleanup.mjs"
    ;;
  *) echo 'Use setup, setup-release or cleanup' >&2; exit 2 ;;
esac
