#!/usr/bin/env bash
# extract-pg-from-docker.sh — Extract portable PostgreSQL binaries from the official Docker image.
#
# Usage: ./scripts/extract-pg-from-docker.sh [--pg-version 17] [--platform linux/amd64] [--output ./dist]
#
# Requires: crane (go-containerregistry) and patchelf
#
# NOTE: ICU libraries are bundled for full collation support.
# This adds ~30MB to the archive. If size is critical, a future
# version could strip ICU data or use --without-icu builds.

set -euo pipefail

PG_MAJOR="${PG_MAJOR:-17}"
PLATFORM="${PLATFORM:-linux/amd64}"
OUTPUT_DIR="${OUTPUT_DIR:-./dist}"
IMAGE="postgres:${PG_MAJOR}"

# Derive arch label from platform string (e.g. linux/amd64 -> amd64)
ARCH_LABEL="${PLATFORM#*/}"
case "$ARCH_LABEL" in
  amd64)  RUST_ARCH="x86_64"  ; DEB_ARCH="x86_64-linux-gnu" ;;
  arm64)  RUST_ARCH="aarch64" ; DEB_ARCH="aarch64-linux-gnu" ;;
  *)      echo "ERROR: Unsupported platform $PLATFORM" >&2; exit 1 ;;
esac

TARGET="${RUST_ARCH}-unknown-linux-gnu"
ARCHIVE_NAME="postgresql-${PG_MAJOR}-${TARGET}.tar.gz"

echo "=== Extracting PostgreSQL ${PG_MAJOR} for ${PLATFORM} ==="
echo "Image:   ${IMAGE}"
echo "Target:  ${TARGET}"
echo "Output:  ${OUTPUT_DIR}/${ARCHIVE_NAME}"

# Working directories
WORK_DIR="$(mktemp -d)"
ROOTFS="${WORK_DIR}/rootfs"
BUNDLE="${WORK_DIR}/bundle"
mkdir -p "${ROOTFS}" "${BUNDLE}"

cleanup() {
  rm -rf "${WORK_DIR}"
}
trap cleanup EXIT

# --- Step 1: Export image filesystem ---
echo "--- Step 1: Exporting image layers (crane) ---"
crane export --platform "${PLATFORM}" "${IMAGE}" - | \
  tar -x -C "${ROOTFS}" \
    "usr/lib/postgresql/${PG_MAJOR}" \
    "usr/share/postgresql/${PG_MAJOR}" \
    "usr/lib/${DEB_ARCH}" \
    "lib/${DEB_ARCH}" \
  2>/dev/null || true

# Verify core binary exists
if [ ! -f "${ROOTFS}/usr/lib/postgresql/${PG_MAJOR}/bin/postgres" ]; then
  echo "ERROR: postgres binary not found in extracted image" >&2
  exit 1
fi

# --- Step 2: Assemble portable bundle ---
echo "--- Step 2: Assembling portable bundle ---"

# Core PostgreSQL directories
cp -a "${ROOTFS}/usr/lib/postgresql/${PG_MAJOR}/bin"   "${BUNDLE}/bin"
cp -a "${ROOTFS}/usr/lib/postgresql/${PG_MAJOR}/lib"   "${BUNDLE}/lib"
cp -a "${ROOTFS}/usr/share/postgresql/${PG_MAJOR}"     "${BUNDLE}/share"

# --- Step 3: Bundle required shared libraries ---
echo "--- Step 3: Bundling shared libraries ---"

# Collect all needed shared libraries from the postgres binary
NEEDED_LIBS=""
for bin in "${BUNDLE}/bin/postgres" "${BUNDLE}/bin/initdb" "${BUNDLE}/bin/pg_ctl" "${BUNDLE}/bin/psql"; do
  if [ -f "$bin" ]; then
    NEEDED_LIBS="${NEEDED_LIBS} $(ldd "$bin" 2>/dev/null | grep -oP '/\S+\.so[.\d]*' || true)"
  fi
done

# Also scan extension .so files in lib/
for ext_so in "${BUNDLE}/lib/"*.so 2>/dev/null; do
  if [ -f "$ext_so" ]; then
    NEEDED_LIBS="${NEEDED_LIBS} $(ldd "$ext_so" 2>/dev/null | grep -oP '/\S+\.so[.\d]*' || true)"
  fi
done

# Copy unique libs that aren't part of base system (skip libc, libm, libdl, libpthread, ld-linux)
SKIP_PATTERN="libc\.so|libm\.so|libdl\.so|libpthread\.so|librt\.so|ld-linux|libnss|libresolv|libcrypt\.so"
echo "$NEEDED_LIBS" | tr ' ' '\n' | sort -u | grep -vE "$SKIP_PATTERN" | while read -r lib; do
  if [ -n "$lib" ] && [ -f "$lib" ]; then
    cp -L "$lib" "${BUNDLE}/lib/" 2>/dev/null || true
  fi
done

# Also look in the extracted rootfs for libs not on the host
for lib_dir in "${ROOTFS}/usr/lib/${DEB_ARCH}" "${ROOTFS}/lib/${DEB_ARCH}"; do
  if [ -d "$lib_dir" ]; then
    # Key libraries to bundle: ssl, crypto, icu, xml2, zstd, lz4, readline
    for pattern in libssl libcrypto libicu libxml2 libzstd liblz4 libreadline libncurses libtinfo libgssapi libkrb5 libk5crypto libcom_err; do
      find "$lib_dir" -name "${pattern}*" -type f -o -name "${pattern}*" -type l 2>/dev/null | while read -r lib; do
        cp -aL "$lib" "${BUNDLE}/lib/" 2>/dev/null || true
      done
    done
  fi
done

# --- Step 4: Patch RPATH ---
echo "--- Step 4: Patching RPATH ---"

if command -v patchelf >/dev/null 2>&1; then
  for bin in "${BUNDLE}/bin/"*; do
    if [ -f "$bin" ] && file "$bin" | grep -q "ELF"; then
      patchelf --set-rpath '$ORIGIN/../lib' "$bin" 2>/dev/null || true
    fi
  done
  for lib in "${BUNDLE}/lib/"*.so*; do
    if [ -f "$lib" ] && file "$lib" | grep -q "ELF"; then
      patchelf --set-rpath '$ORIGIN' "$lib" 2>/dev/null || true
    fi
  done
  echo "RPATH patched successfully"
else
  echo "WARNING: patchelf not found — binaries may not be portable" >&2
fi

# --- Step 5: Strip debug symbols (size optimization) ---
echo "--- Step 5: Stripping debug symbols ---"
find "${BUNDLE}" -type f \( -name "*.so*" -o -perm /111 \) -exec strip --strip-debug {} \; 2>/dev/null || true

# --- Step 6: Write marker and metadata ---
echo "--- Step 6: Writing metadata ---"
PG_FULL_VERSION=$("${BUNDLE}/bin/postgres" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+' || echo "${PG_MAJOR}")
echo "${PG_FULL_VERSION}" > "${BUNDLE}/.pg_version"
touch "${BUNDLE}/.extracted"

# Report sizes
echo ""
echo "=== Bundle contents ==="
du -sh "${BUNDLE}/bin" "${BUNDLE}/lib" "${BUNDLE}/share" 2>/dev/null || true
ICU_SIZE=$(du -sh "${BUNDLE}/lib/"libicu* 2>/dev/null | tail -1 | cut -f1 || echo "0")
echo "ICU libraries: ~${ICU_SIZE}"
echo ""

# --- Step 7: Package ---
echo "--- Step 7: Packaging ---"
mkdir -p "${OUTPUT_DIR}"
tar -czf "${OUTPUT_DIR}/${ARCHIVE_NAME}" -C "${BUNDLE}" .

ARCHIVE_SIZE=$(du -sh "${OUTPUT_DIR}/${ARCHIVE_NAME}" | cut -f1)
echo "=== Done ==="
echo "Archive: ${OUTPUT_DIR}/${ARCHIVE_NAME} (${ARCHIVE_SIZE})"
echo "SHA256:  $(sha256sum "${OUTPUT_DIR}/${ARCHIVE_NAME}" | cut -d' ' -f1)"
