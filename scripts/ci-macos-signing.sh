#!/bin/bash
set -euo pipefail
[[ "${GITHUB_ACTIONS:-}" == true && "${RUNNER_ENVIRONMENT:-}" == github-hosted && "${RUNNER_OS:-}" == macOS ]]
: "${RUNNER_TEMP:?}"
CI_SIGNING_ROOT="$RUNNER_TEMP/vigilshell-ci-signing"
CI_KEYCHAIN="$CI_SIGNING_ROOT/ci.keychain-db"
case "${1:-}" in
  setup|setup-release)
    if [[ "$1" == setup-release ]]; then
      node "$(dirname "$0")/release-draft.mjs" verify-source
    fi
    mkdir -m 700 "$CI_SIGNING_ROOT"
    umask 077
    # Exported keys are never needed after import, including on setup failure.
    trap 'rm -f "$CI_SIGNING_ROOT/key.pem" "$CI_SIGNING_ROOT/identity.p12"' EXIT
    security list-keychains -d user > "$CI_SIGNING_ROOT/keychains-before.txt"
    CI_PASSWORD="$(openssl rand -hex 32)"
    echo "::add-mask::$CI_PASSWORD"
    if [[ "$1" == setup-release ]]; then
      : "${MACOS_SIGNING_P12_BASE64:?Missing release signing certificate}"
      : "${MACOS_SIGNING_P12_PASSWORD:?Missing release signing certificate password}"
      : "${MACOS_SIGNING_IDENTITY:?Missing fixed release signing fingerprint}"
      printf '%s' "$MACOS_SIGNING_P12_BASE64" | /usr/bin/base64 -D > "$CI_SIGNING_ROOT/identity.p12"
      openssl pkcs12 -in "$CI_SIGNING_ROOT/identity.p12" -clcerts -nokeys \
        -passin env:MACOS_SIGNING_P12_PASSWORD -out "$CI_SIGNING_ROOT/certificate.pem"
      IMPORT_PASSWORD="$MACOS_SIGNING_P12_PASSWORD"
    else
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
    fi
    CI_IDENTITY="$(openssl x509 -in "$CI_SIGNING_ROOT/certificate.pem" -noout -fingerprint -sha1 | cut -d= -f2 | tr -d ':')"
    if [[ "$1" == setup-release ]]; then
      [[ "$CI_IDENTITY" == "$(printf '%s' "$MACOS_SIGNING_IDENTITY" | tr '[:lower:]' '[:upper:]')" ]] || {
        echo 'Imported certificate does not match the configured release identity' >&2
        exit 1
      }
    fi
    unset MACOS_SIGNING_P12_BASE64 MACOS_SIGNING_P12_PASSWORD
    security create-keychain -p "$CI_PASSWORD" "$CI_KEYCHAIN"
    security set-keychain-settings -lut 21600 "$CI_KEYCHAIN"
    security unlock-keychain -p "$CI_PASSWORD" "$CI_KEYCHAIN"
    security import "$CI_SIGNING_ROOT/identity.p12" -k "$CI_KEYCHAIN" -P "$IMPORT_PASSWORD" -T /usr/bin/codesign
    rm -f "$CI_SIGNING_ROOT/key.pem" "$CI_SIGNING_ROOT/identity.p12"
    echo "Authorizing CI signing key"
    security set-key-partition-list -S apple-tool:,apple: -s -k "$CI_PASSWORD" "$CI_KEYCHAIN" >/dev/null
    echo "Trusting CI signing certificate"
    sudo -n security add-trusted-cert -d -r trustRoot -p codeSign -k "$CI_KEYCHAIN" "$CI_SIGNING_ROOT/certificate.pem"
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
