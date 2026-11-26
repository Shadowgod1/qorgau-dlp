#!/bin/bash
set -euo pipefail

DEBIAN_ISO_URL="${DEBIAN_ISO_URL:-https://cdimage.debian.org/cdimage/archive/latest-oldstable/amd64/iso-cd/}"
CACHE_DIR="/work/cache"
OUT_DIR="/work/dist"
PAYLOAD_DIR="/work/payload"
ISO_WORK="/tmp/iso_extract"

mkdir -p "$CACHE_DIR" "$OUT_DIR" "$ISO_WORK"

# 1. Resolve & cache latest Debian 12 netinst ISO
BASE_ISO="$CACHE_DIR/debian-netinst.iso"
if [ ! -f "$BASE_ISO" ]; then
    echo ">>> Resolving latest Debian amd64 netinst ISO..."
    ISO_NAME=$(curl -fsSL "$DEBIAN_ISO_URL" | grep -oE 'debian-[0-9]+\.[0-9]+\.[0-9]+-amd64-netinst\.iso' | head -n 1)
    if [ -z "$ISO_NAME" ]; then
        echo "ERROR: Could not detect Debian netinst ISO filename from $DEBIAN_ISO_URL"
        exit 1
    fi
    echo ">>> Downloading $ISO_NAME ..."
    curl -fL --progress-bar "${DEBIAN_ISO_URL}${ISO_NAME}" -o "$BASE_ISO"
fi

# 2. Download offline .deb packages (PostgreSQL + Nginx + Tesseract rus/kaz/eng)
DEB_CACHE="$CACHE_DIR/debs"
if [ ! -d "$DEB_CACHE" ] || [ -z "$(ls -A "$DEB_CACHE" 2>/dev/null)" ]; then
    echo ">>> Downloading offline .deb dependencies (PostgreSQL, Nginx, Tesseract OCR)..."
    mkdir -p "$DEB_CACHE"
    apt-get update
    apt-get install -y --download-only --no-install-recommends \
        -o Dir::Cache::archives="$DEB_CACHE" \
        postgresql nginx-light openssl tesseract-ocr tesseract-ocr-rus tesseract-ocr-kaz tesseract-ocr-eng
    rm -rf "$DEB_CACHE/partial" "$DEB_CACHE/lock"
fi

# 3. Extract base Debian ISO
echo ">>> Extracting Debian netinst ISO..."
rm -rf "$ISO_WORK"
mkdir -p "$ISO_WORK"
xorriso -osirrox on -indev "$BASE_ISO" -extract / "$ISO_WORK"
chmod -R +w "$ISO_WORK"

# 4. Copy Qorgau DLP payload onto ISO (/qorgau)
echo ">>> Embedding Qorgau DLP v2.0 server, dashboard, agent, and offline packages..."
mkdir -p "$ISO_WORK/qorgau/bin" "$ISO_WORK/qorgau/dashboard" "$ISO_WORK/qorgau/downloads" "$ISO_WORK/qorgau/debs"
cp "$PAYLOAD_DIR/qorgau-server" "$ISO_WORK/qorgau/bin/qorgau-server"
cp -a "$PAYLOAD_DIR/dashboard/." "$ISO_WORK/qorgau/dashboard/"
if [ -f "$PAYLOAD_DIR/qorgau-agent.exe" ]; then
    cp "$PAYLOAD_DIR/qorgau-agent.exe" "$ISO_WORK/qorgau/downloads/qorgau-agent.exe"
fi
cp "$DEB_CACHE"/*.deb "$ISO_WORK/qorgau/debs/"
cp /work/iso/nginx-native.conf "$ISO_WORK/qorgau/nginx-native.conf"
cp /work/iso/qorgau-server.service "$ISO_WORK/qorgau/qorgau-server.service"
cp /work/iso/post-install.sh "$ISO_WORK/qorgau/post-install.sh"
sed -i 's/\r$//' "$ISO_WORK/qorgau/post-install.sh" "$ISO_WORK/qorgau/nginx-native.conf" "$ISO_WORK/qorgau/qorgau-server.service"

# 5. Inject preseed.cfg directly into initrd.gz for zero-touch automated boot
echo ">>> Injecting preseed.cfg into initrd.gz..."
PRESEED_TMP="/tmp/preseed_inject"
rm -rf "$PRESEED_TMP" && mkdir -p "$PRESEED_TMP"
cp /work/iso/preseed.cfg "$PRESEED_TMP/preseed.cfg"
sed -i 's/\r$//' "$PRESEED_TMP/preseed.cfg"

gunzip "$ISO_WORK/install.amd/initrd.gz"
(cd "$PRESEED_TMP" && echo preseed.cfg | cpio -H newc -o -A -F "$ISO_WORK/install.amd/initrd")
gzip -9 "$ISO_WORK/install.amd/initrd"

# 6. Configure ISOLINUX (BIOS) & GRUB (UEFI) for automatic Qorgau DLP installation
cat >"$ISO_WORK/isolinux/isolinux.cfg" <<'EOF'
DEFAULT qorgau
TIMEOUT 30
PROMPT 0
LABEL qorgau
  MENU LABEL Install Qorgau DLP v2.0 Appliance (Automated)
  KERNEL /install.amd/vmlinuz
  APPEND vga=788 initrd=/install.amd/initrd.gz auto=true priority=critical preseed/file=/preseed.cfg --- quiet
EOF

cat >"$ISO_WORK/boot/grub/grub.cfg" <<'EOF'
set default=0
set timeout=3
menuentry "Install Qorgau DLP v2.0 Appliance (Automated)" {
    linux  /install.amd/vmlinuz auto=true priority=critical preseed/file=/preseed.cfg --- quiet
    initrd /install.amd/initrd.gz
}
EOF

# Update md5sum.txt
(cd "$ISO_WORK" && find . -type f ! -name md5sum.txt -print0 | xargs -0 md5sum > md5sum.txt)

# 7. Pack hybrid BIOS + UEFI bootable ISO
OUT_ISO="$OUT_DIR/qorgau-dlp-2.0-amd64.iso"
echo ">>> Building bootable hybrid ISO -> $OUT_ISO ..."
xorriso -as mkisofs \
    -r -V "QORGAU_DLP_2_0" \
    -o "$OUT_ISO" \
    -J -joliet-long \
    -isohybrid-mbr /usr/lib/ISOLINUX/isohdpfx.bin \
    -c isolinux/boot.cat \
    -b isolinux/isolinux.bin \
    -no-emul-boot -boot-load-size 4 -boot-info-table \
    -eltorito-alt-boot \
    -e boot/grub/efi.img \
    -no-emul-boot -isohybrid-gpt-basdat \
    "$ISO_WORK"

sha256sum "$OUT_ISO" > "${OUT_ISO}.sha256"
ls -lh "$OUT_ISO"
cat "${OUT_ISO}.sha256"
