#!/bin/bash -p
set -euo pipefail

# Immediate rejection of loader injection
if [ -n "${LD_PRELOAD:-}" ] || [ -n "${LD_LIBRARY_PATH:-}" ]; then
    echo "Security Error: Prohibited loader control variable detected" >&2
    exit 1
fi

# Re-exec through fixed trusted shell under sanitized environment (/usr/bin/env -i)
if [ "${OMANOTES_SECURE_ENV:-0}" != "1" ]; then
    exec /usr/bin/env -i \
        PATH="/usr/bin:/bin:${HOME}/.local/bin:${HOME}/.cargo/bin" \
        HOME="${HOME}" \
        USER="${USER:-$(/usr/bin/id -un 2>/dev/null || echo "user")}" \
        LANG="${LANG:-C.UTF-8}" \
        LC_ALL="${LC_ALL:-C.UTF-8}" \
        OMANOTES_SECURE_ENV=1 \
        /bin/bash -p "$0" "$@"
fi

# Reject unexpected build and loader controls before building
for var in LD_PRELOAD LD_LIBRARY_PATH RUSTC_WRAPPER CARGO_BUILD_RUSTC_WRAPPER RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_HOME BASH_ENV ENV; do
    if [ -n "${!var+x}" ]; then
        echo "Security Error: Prohibited build/loader control variable detected: ${var}" >&2
        exit 1
    fi
done

DIR="$(cd "$(/usr/bin/dirname "$(/usr/bin/realpath "${BASH_SOURCE[0]}")")" && /usr/bin/pwd)"
cd "$DIR"

CARGO_BIN=""
if [[ -x /usr/bin/cargo ]]; then
    CARGO_BIN="/usr/bin/cargo"
elif [[ -x "${HOME}/.cargo/bin/cargo" ]]; then
    CARGO_BIN="${HOME}/.cargo/bin/cargo"
else
    echo "Error: cargo binary not found" >&2
    exit 1
fi

echo "Building omanotes-engine from source..."
TMP_BUILD_DIR="$(/usr/bin/mktemp -d -t omanotes-build.XXXXXX)"
cleanup() {
    /usr/bin/rm -rf "${TMP_BUILD_DIR}"
}
trap cleanup EXIT

"${CARGO_BIN}" build --release --locked --target-dir "${TMP_BUILD_DIR}"
/usr/bin/install -m 755 "${TMP_BUILD_DIR}/release/omanotes-engine" "${DIR}/omanotes-engine"

# Bind source tree identity and installed binary SHA-256
SOURCE_HASH=""
if [[ -d "${DIR}/src" && -f "${DIR}/Cargo.toml" && -f "${DIR}/Cargo.lock" ]]; then
    SOURCE_HASH="$(/usr/bin/find "${DIR}/src" "${DIR}/Cargo.toml" "${DIR}/Cargo.lock" -type f 2>/dev/null | /usr/bin/sort | /usr/bin/xargs /usr/bin/sha256sum 2>/dev/null | /usr/bin/sha256sum | /usr/bin/awk '{print $1}')"
fi

if [[ -z "${SOURCE_HASH}" ]]; then
    echo "Security Error: Unable to compute source identity" >&2
    exit 1
fi

BIN_HASH="$(/usr/bin/sha256sum "${DIR}/omanotes-engine" 2>/dev/null | /usr/bin/awk '{print $1}')"
if [[ -z "${BIN_HASH}" ]]; then
    echo "Security Error: Unable to compute engine binary digest" >&2
    exit 1
fi

echo "${SOURCE_HASH} ${BIN_HASH}" > "${DIR}/.engine-provenance"

# User state directory and installation manifest
STATE_DIR="${HOME}/.local/state/omarchy/omanotes"
MANIFEST_FILE="${STATE_DIR}/install_manifest.json"
/usr/bin/install -d -m 700 "${STATE_DIR}"
/usr/bin/install -d -m 755 "${HOME}/.local/bin"

# Binaries to install
TARGET_BINARIES=("omanotes-engine" "omanotes" "omanotes-dashboard" "omanotes-status")

get_manifest_hash() {
    local target="$1"
    /usr/bin/python3 -c '
import json, sys
target = sys.argv[1]
manifest_path = sys.argv[2]
try:
    with open(manifest_path, "r", encoding="utf-8") as f:
        data = json.load(f)
        files = data.get("files", {})
        if target in files:
            print(files[target])
            sys.exit(0)
        else:
            sys.exit(2)
except Exception:
    sys.exit(3)
' "${target}" "${MANIFEST_FILE}"
}

# Pre-installation ownership verification:
# Refuse to overwrite foreign files, symlinks, or tampered files not matching OmaNotes manifest.
for bin_name in "${TARGET_BINARIES[@]}"; do
    target_path="${HOME}/.local/bin/${bin_name}"
    if [[ -e "${target_path}" || -L "${target_path}" ]]; then
        if [[ -L "${target_path}" ]]; then
            echo "Security Error: Target ${target_path} is a symlink. Refusing to overwrite foreign or symlinked file." >&2
            exit 1
        fi

        if [[ ! -f "${target_path}" ]]; then
            echo "Security Error: Target ${target_path} is not a regular file." >&2
            exit 1
        fi

        # If file exists, verify ownership and integrity via manifest
        if [[ -f "${MANIFEST_FILE}" ]]; then
            recorded_hash=""
            manifest_status=0
            recorded_hash=$(get_manifest_hash "${target_path}") || manifest_status=$?

            if [[ ${manifest_status} -eq 3 ]]; then
                echo "Security Error: Installation manifest at ${MANIFEST_FILE} is corrupt or unreadable." >&2
                exit 1
            elif [[ ${manifest_status} -eq 2 || -z "${recorded_hash}" ]]; then
                echo "Security Conflict: A pre-existing non-OmaNotes file exists at ${target_path}." >&2
                echo "Refusing to overwrite foreign user-managed executable." >&2
                exit 1
            elif [[ ${manifest_status} -ne 0 ]]; then
                echo "Security Error: Failed to query installation manifest." >&2
                exit 1
            fi

            current_hash=$(/usr/bin/sha256sum "${target_path}" 2>/dev/null | /usr/bin/awk '{print $1}')
            if [[ -z "${current_hash}" || "${current_hash}" != "${recorded_hash}" ]]; then
                echo "Security Conflict: File ${target_path} hash does not match installation manifest." >&2
                echo "Refusing to overwrite modified or untracked file." >&2
                exit 1
            fi
        else
            echo "Security Conflict: Target ${target_path} exists but no installation manifest was found." >&2
            echo "Refusing to overwrite untracked pre-existing file." >&2
            exit 1
        fi
    fi
done

# Install user-facing binaries in ~/.local/bin
/usr/bin/install -m 755 "${DIR}/omanotes-engine" "${HOME}/.local/bin/omanotes-engine"
/usr/bin/install -m 755 "${DIR}/omanotes-engine" "${HOME}/.local/bin/omanotes"
/usr/bin/install -m 755 "${DIR}/omanotes-dashboard" "${HOME}/.local/bin/omanotes-dashboard"
/usr/bin/install -m 755 "${DIR}/omanotes-status" "${HOME}/.local/bin/omanotes-status"

# Record install manifest with SHA-256 hashes to track exact files created by this installer
TMP_MANIFEST="$(/usr/bin/mktemp -p "${STATE_DIR}" .tmp_manifest.XXXXXX)"
chmod 600 "${TMP_MANIFEST}"

{
    echo "{"
    echo "  \"installer\": \"ozdil.omanotes\","
    echo "  \"installed_at\": \"$(date -u +"%Y-%m-%dT%H:%M:%SZ")\","
    echo "  \"files\": {"
    first=true
    for bin_name in "${TARGET_BINARIES[@]}"; do
        target_path="${HOME}/.local/bin/${bin_name}"
        sha=$(/usr/bin/sha256sum "${target_path}" | /usr/bin/awk '{print $1}')
        if [ "$first" = true ]; then
            first=false
        else
            echo ","
        fi
        printf '    "%s": "%s"' "${target_path}" "${sha}"
    done
    echo ""
    echo "  }"
    echo "}"
} > "${TMP_MANIFEST}"

mv -f "${TMP_MANIFEST}" "${MANIFEST_FILE}"
chmod 600 "${MANIFEST_FILE}"

echo "omanotes-engine successfully compiled and recorded in install manifest."

if [[ -x /usr/bin/omarchy-restart-shell ]]; then
    echo "Reloading Omarchy shell..."
    /usr/bin/omarchy-restart-shell >/dev/null 2>&1 || true
fi

echo "OmaNotes setup is complete and ready."

