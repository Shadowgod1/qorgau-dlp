#!/bin/bash
set -euo pipefail

echo "=== Qorgau DLP v2.0 Appliance Offline Setup ==="

# 1. Install offline .deb dependencies (PostgreSQL, Nginx, Tesseract OCR rus/kaz/eng)
if [ -d /opt/qorgau/debs ] && ls /opt/qorgau/debs/*.deb >/dev/null 2>&1; then
    dpkg -i --force-depends /opt/qorgau/debs/*.deb || true
    apt-get -f install -y --no-download || true
fi

# 2. Create service user & storage directories
id -u qorgau >/dev/null 2>&1 || useradd -r -s /usr/sbin/nologin -d /var/lib/qorgau qorgau
mkdir -p /var/lib/qorgau/{screenshots,shadow,audio,webcam_photos,downloads} /etc/qorgau
cp /opt/qorgau/bin/qorgau-server /usr/local/bin/qorgau-server
chmod 755 /usr/local/bin/qorgau-server
if [ -f /opt/qorgau/downloads/qorgau-agent.exe ]; then
    cp /opt/qorgau/downloads/qorgau-agent.exe /var/lib/qorgau/downloads/qorgau-agent.exe
fi
chown -R qorgau:qorgau /var/lib/qorgau

# 3. Generate unique per-appliance random secrets
DB_PASS=$(tr -dc 'A-Za-z0-9' </dev/urandom | head -c 24)
JWT_SECRET=$(tr -dc 'A-Za-z0-9' </dev/urandom | head -c 48)
AGENT_TOK=$(tr -dc 'A-Za-z0-9' </dev/urandom | head -c 32)
ADMIN_PASS="qorgau_admin_2026"

cat >/etc/qorgau/qorgau.env <<EOF
BIND_ADDR=127.0.0.1:8000
DATABASE_URL=postgres://qorgau_user:${DB_PASS}@127.0.0.1:5432/qorgau_dlp
SECRET_KEY=${JWT_SECRET}
JWT_EXPIRE_MIN=480
AGENT_TOKEN=${AGENT_TOK}
CORS_ORIGINS=http://localhost,https://localhost
ADMIN_USERNAME=admin
ADMIN_PASSWORD=${ADMIN_PASS}
SCREENSHOTS_DIR=/var/lib/qorgau/screenshots
SHADOW_DIR=/var/lib/qorgau/shadow
AUDIO_DIR=/var/lib/qorgau/audio
WEBCAM_DIR=/var/lib/qorgau/webcam_photos
AGENT_BIN_PATH=/var/lib/qorgau/downloads/qorgau-agent.exe
SCREENSHOTS_RETENTION_DAYS=30
SHADOW_RETENTION_DAYS=365
AUDIO_RETENTION_DAYS=30
WEBCAM_RETENTION_DAYS=30
DATA_RETENTION_DAYS=365
OCR_CONCURRENCY=2
EOF
chmod 600 /etc/qorgau/qorgau.env
chown qorgau:qorgau /etc/qorgau/qorgau.env

# 4. Initialize PostgreSQL database
systemctl enable postgresql || true
service postgresql start || true
su - postgres -c "psql -tc \"SELECT 1 FROM pg_roles WHERE rolname='qorgau_user'\" | grep -q 1 || psql -c \"CREATE USER qorgau_user WITH PASSWORD '${DB_PASS}';\""
su - postgres -c "psql -tc \"SELECT 1 FROM pg_database WHERE datname='qorgau_dlp'\" | grep -q 1 || psql -c \"CREATE DATABASE qorgau_dlp OWNER qorgau_user;\""

# 5. Generate self-signed TLS certificate & configure Nginx + systemd
mkdir -p /etc/qorgau/tls
if command -v openssl >/dev/null 2>&1; then
    openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
        -keyout /etc/qorgau/tls/qorgau.key \
        -out /etc/qorgau/tls/qorgau.crt \
        -subj "/C=KZ/O=Qorgau DLP/CN=qorgau-dlp.local" \
        -addext "subjectAltName=DNS:localhost,DNS:qorgau-dlp.local,IP:127.0.0.1" >/dev/null 2>&1 || true
    chmod 600 /etc/qorgau/tls/qorgau.key
    chmod 644 /etc/qorgau/tls/qorgau.crt
fi

cp /opt/qorgau/nginx-native.conf /etc/nginx/sites-available/qorgau
rm -f /etc/nginx/sites-enabled/default
ln -sf /etc/nginx/sites-available/qorgau /etc/nginx/sites-enabled/qorgau
cp /opt/qorgau/qorgau-server.service /etc/systemd/system/qorgau-server.service
systemctl enable nginx qorgau-server || true

# 6. Console welcome banner with agent token & login info
cat >/etc/issue <<EOF

============================================================
  QORGAU DLP v2.0 Appliance (Rust Edition)
  Web Console : https://\4/  (HTTP: http://\4/)
  Login       : admin / ${ADMIN_PASS}
  Agent Token : ${AGENT_TOK}
  Config File : /etc/qorgau/qorgau.env
  TLS Cert    : /etc/qorgau/tls/qorgau.crt
============================================================

EOF

echo "=== Qorgau DLP v2.0 Appliance Setup Complete ==="
