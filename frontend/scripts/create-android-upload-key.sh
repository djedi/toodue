#!/usr/bin/env bash
# Creates the Android upload keystore used to sign Play Store bundles and
# appends its settings to fastlane/.env. Google Play App Signing holds the real
# app signing key; this upload key can be reset via Play support if lost, but
# back it up (1Password) anyway.
set -euo pipefail

cd "$(dirname "$0")/.."
store="${1:-$HOME/.play/toodue-upload.jks}"
env_file="fastlane/.env"

if [[ -e "$store" ]]; then
  echo "Keystore already exists: $store" >&2
  exit 1
fi
mkdir -p "$(dirname "$store")"
chmod 700 "$(dirname "$store")"

password="$(openssl rand -base64 30 | tr -d '/+=' | cut -c1-32)"
keytool -genkeypair -v -keystore "$store" -storetype PKCS12 \
  -alias upload -keyalg RSA -keysize 4096 -validity 10000 \
  -storepass "$password" -keypass "$password" \
  -dname "CN=TooDue, O=Inzolo LLC, C=US" >/dev/null 2>&1
chmod 600 "$store"

touch "$env_file"
grep -v '^TOODUE_UPLOAD_' "$env_file" > "$env_file.tmp" || true
cat >> "$env_file.tmp" <<EOF
TOODUE_UPLOAD_STORE_FILE=$store
TOODUE_UPLOAD_STORE_PASSWORD=$password
TOODUE_UPLOAD_KEY_ALIAS=upload
TOODUE_UPLOAD_KEY_PASSWORD=$password
EOF
mv "$env_file.tmp" "$env_file"
chmod 600 "$env_file"

echo "Created $store and wrote its settings to $env_file."
echo "Back up both the keystore and the password; you need them for every Play upload."
