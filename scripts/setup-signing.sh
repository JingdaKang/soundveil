#!/bin/bash
set -euo pipefail

# Creates the local code-signing identity used by build.sh. macOS stores the signing
# identity with the recording permission, so a stable certificate keeps that permission
# across builds. Ad-hoc signatures change every build. The key stays in the login keychain.
NAME="Soundveil Local Signing"
if security find-certificate -c "$NAME" >/dev/null 2>&1; then
 echo "Signing identity already exists: $NAME"; exit 0
fi
WORK=$(mktemp -d); trap 'rm -rf "$WORK"' EXIT
PASS=$(/usr/bin/openssl rand -hex 16)
cat > "$WORK/cert.conf" <<CONF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
CONF
/usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config "$WORK/cert.conf" \
 -keyout "$WORK/key.pem" -out "$WORK/cert.pem" 2>/dev/null
/usr/bin/openssl pkcs12 -export -inkey "$WORK/key.pem" -in "$WORK/cert.pem" \
 -name "$NAME" -passout "pass:$PASS" -out "$WORK/identity.p12"
security import "$WORK/identity.p12" -k login.keychain-db -P "$PASS" -T /usr/bin/codesign >/dev/null
echo "Created signing identity: $NAME"
