#!/usr/bin/env bash
set -euo pipefail

PG_MAJOR="${PG_MAJOR:-17}"
PLATFORM="${PLATFORM:-linux/amd64}"
OUTPUT_DIR="${OUTPUT_DIR:-./dist}"
ENGINE="${ENGINE:-postgresql}"

ARCH_LABEL="${PLATFORM#*/}"
case "$ARCH_LABEL" in
  amd64)  RUST_ARCH="x86_64"  ; DEB_ARCH="x86_64-linux-gnu" ;;
  arm64)  RUST_ARCH="aarch64" ; DEB_ARCH="aarch64-linux-gnu" ;;
  *)      echo "ERROR: Unsupported platform $PLATFORM" >&2; exit 1 ;;
esac

TARGET="${RUST_ARCH}-unknown-linux-gnu"

if [ "$ENGINE" = "postgresql-spock" ] || [ "$ENGINE" = "spock" ]; then
  IMAGE="ghcr.io/pgedge/pgedge-postgres:${PG_MAJOR}-spock5-standard"
  PG_PATH="usr/pgsql-${PG_MAJOR}"
  ARCHIVE_BASE="postgresql-spock-${PG_MAJOR}-${TARGET}"
else
  IMAGE="postgres:${PG_MAJOR}"
  PG_PATH="usr/lib/postgresql/${PG_MAJOR}"
  if [[ "$ENGINE" == *"without-llvm"* ]]; then
    ARCHIVE_BASE="postgresql-without-llvm-${PG_MAJOR}-${TARGET}"
  else
    ARCHIVE_BASE="postgresql-${PG_MAJOR}-${TARGET}"
  fi
fi

echo "=== Extracting ${ENGINE} ${PG_MAJOR} for ${PLATFORM} ==="
echo "Image:   ${IMAGE}"
echo "Target:  ${TARGET}"

WORK_DIR="$(mktemp -d)"
ROOTFS="${WORK_DIR}/rootfs"
BUNDLE="${WORK_DIR}/bundle"
mkdir -p "${ROOTFS}" "${BUNDLE}"

cleanup() {
  chmod -R u+w "${WORK_DIR}" 2>/dev/null || true
  rm -rf "${WORK_DIR}"
}
# trap cleanup EXIT

echo "--- Step 1: Exporting image layers ---"
echo "Using docker to extract image layers..."
docker pull --platform "${PLATFORM}" "${IMAGE}" >/dev/null
CID=$(docker create --platform "${PLATFORM}" "${IMAGE}" dummy)
docker export "$CID" | tar -x -C "${ROOTFS}" 2>/dev/null || true
docker rm -v "$CID" >/dev/null

if [ ! -f "${ROOTFS}/${PG_PATH}/bin/postgres" ]; then
  echo "ERROR: postgres binary not found in extracted image" >&2
  exit 1
fi

echo "--- Step 2: Assembling portable bundle ---"
mkdir -p "${BUNDLE}"

if [ "$ENGINE" = "postgresql-spock" ] || [ "$ENGINE" = "spock" ]; then
  mkdir -p "${BUNDLE}/${PG_PATH}"
  cp -a "${ROOTFS}/${PG_PATH}/bin"   "${BUNDLE}/${PG_PATH}/bin"
  cp -a "${ROOTFS}/${PG_PATH}/lib"   "${BUNDLE}/${PG_PATH}/lib"
  cp -a "${ROOTFS}/${PG_PATH}/share" "${BUNDLE}/${PG_PATH}/share"
  
  ln -s "${PG_PATH}/bin" "${BUNDLE}/bin"
  ln -s "${PG_PATH}/lib" "${BUNDLE}/lib"
  ln -s "${PG_PATH}/share" "${BUNDLE}/share"
else
  mkdir -p "${BUNDLE}/${PG_PATH}"
  mkdir -p "${BUNDLE}/usr/share/postgresql"
  
  cp -a "${ROOTFS}/${PG_PATH}/bin" "${BUNDLE}/${PG_PATH}/bin"
  cp -a "${ROOTFS}/${PG_PATH}/lib" "${BUNDLE}/${PG_PATH}/lib"
  cp -a "${ROOTFS}/usr/share/postgresql/${PG_MAJOR}" "${BUNDLE}/usr/share/postgresql/${PG_MAJOR}"
  
  ln -s "${PG_PATH}/bin" "${BUNDLE}/bin"
  ln -s "${PG_PATH}/lib" "${BUNDLE}/lib"
  ln -s "usr/share/postgresql/${PG_MAJOR}" "${BUNDLE}/share"
fi

for conf in postgresql.conf.sample pg_hba.conf.sample pg_ident.conf.sample; do
  if [ -L "${BUNDLE}/share/${conf}" ] && [ ! -e "${BUNDLE}/share/${conf}" ]; then
    echo "Fixing broken symlink ${conf}..."
    rm -f "${BUNDLE}/share/${conf}"
    docker run --rm --entrypoint cat "$IMAGE" "/usr/share/postgresql/${conf}" > "${BUNDLE}/share/${conf}"
  fi
done

if [[ "$ENGINE" == *"without-llvm"* ]]; then
  echo "--- Feature Stripping: Removing LLVM/JIT ---"
  find "${BUNDLE}/lib" -name "*llvmjit*.so" -delete
  find "${BUNDLE}/lib" -name "libLLVM*.so*" -delete
  rm -rf "${BUNDLE}/lib/postgresql/bitcode" || true
fi

echo "--- Step 3: Bundling shared libraries ---"
NEEDED_LIBS=""
for bin in "${BUNDLE}/bin/"*; do
  if [ -f "$bin" ] && [ -x "$bin" ]; then
    NEEDED_LIBS="${NEEDED_LIBS} $(ldd "$bin" 2>/dev/null | grep -oP '/\S+\.so[.\d]*' || true)"
  fi
done

for ext_so in "${BUNDLE}/lib/"*.so; do
  if [ -f "$ext_so" ]; then
    NEEDED_LIBS="${NEEDED_LIBS} $(ldd "$ext_so" 2>/dev/null | grep -oP '/\S+\.so[.\d]*' || true)"
  fi
done

SKIP_PATTERN="libc\.so|libm\.so|libdl\.so|libpthread\.so|librt\.so|ld-linux|libnss|libresolv|libcrypt\.so"
echo "$NEEDED_LIBS" | tr ' ' '\n' | sort -u | grep -vE "$SKIP_PATTERN" | while read -r lib; do
  if [ -n "$lib" ] && [ -f "$lib" ]; then
    cp -L "$lib" "${BUNDLE}/lib/" 2>/dev/null || true
  fi
done

for lib_dir in "${ROOTFS}/usr/lib/${DEB_ARCH}" "${ROOTFS}/lib/${DEB_ARCH}" "${ROOTFS}/usr/lib64" "${ROOTFS}/lib64"; do
  if [ -d "$lib_dir" ]; then
    for pattern in libssl libcrypto libicu libxml2 libzstd liblz4 libreadline libncurses libtinfo libgssapi libkrb5 libk5crypto libcom_err; do
      find "$lib_dir" -name "${pattern}*" -type f -o -name "${pattern}*" -type l 2>/dev/null | while read -r lib; do
        cp -aL "$lib" "${BUNDLE}/lib/" 2>/dev/null || true
      done
    done
  fi
done

echo "--- Step 4: Patching RPATH ---"
find "${BUNDLE}/bin/" "${BUNDLE}/lib/" -type f -executable -exec sh -c '
  for f do
    if patchelf --print-rpath "$f" >/dev/null 2>&1; then
      patchelf --force-rpath --set-rpath "\$ORIGIN/../lib" "$f"
    fi
  done
' sh {} +

echo "RPATH patched successfully"

echo "--- Step 5: Stripping debug symbols ---"
find "${BUNDLE}/bin/" "${BUNDLE}/lib/" -type f -exec strip --strip-unneeded {} 2>/dev/null \;

echo "--- Step 6: Writing metadata ---"
PG_FULL_VERSION=$("${BUNDLE}/bin/postgres" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+' || echo "${PG_MAJOR}")
echo "${PG_FULL_VERSION}" > "${BUNDLE}/.pg_version"
touch "${BUNDLE}/.extracted"

echo ""
echo "=== Bundle contents ==="
du -sh "${BUNDLE}/bin" "${BUNDLE}/lib" "${BUNDLE}/share" 2>/dev/null || true
ICU_SIZE=$(du -sh "${BUNDLE}/lib/"libicu* 2>/dev/null | tail -1 | cut -f1 || echo "0")
echo "ICU libraries: ~${ICU_SIZE}"
echo ""

echo "--- Step 7: Packaging ---"
mkdir -p "${OUTPUT_DIR}"
TEMP_ARCHIVE="${OUTPUT_DIR}/.tmp_${ARCHIVE_BASE}.tar.gz"
tar -czf "${TEMP_ARCHIVE}" -C "${BUNDLE}" .

SIZE_MB=$(du -m "${TEMP_ARCHIVE}" | cut -f1)
FINAL_ARCHIVE="${OUTPUT_DIR}/${ARCHIVE_BASE}-${SIZE_MB}M.tar.gz"
mv "${TEMP_ARCHIVE}" "${FINAL_ARCHIVE}"

echo "=== Done ==="
echo "Archive: ${FINAL_ARCHIVE}"
echo "SHA256:  $(sha256sum "${FINAL_ARCHIVE}" | cut -d' ' -f1)"
