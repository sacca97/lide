#!/usr/bin/env bash
#
# Report or remove stale Cargo artifacts.
#
# Cargo never garbage-collects old build-script directories. On a long-lived
# checkout they can grow to tens of gigabytes even though only the newest
# hash is ever used. This script removes:
#
#   * temporary linker output (`*.tmp*`) under the target directory that is
#     older than an hour — what a killed build leaves behind;
#   * build-script output directories under <target>/<profile>/build that
#     have not been touched for --days days;
#   * incremental-compilation caches under <target>/<profile>/incremental
#     that are older than --days;
#   * downloaded crate archives under $CARGO_HOME/registry/cache older than
#     --days (Cargo downloads them again when needed).
#
# It does not touch compiled libraries, the extracted registry sources, or
# any project's source tree.
#
# Usage:
#   scripts/clean-cargo-cache.sh                  # report only
#   scripts/clean-cargo-cache.sh --apply          # delete what it reported
#   scripts/clean-cargo-cache.sh --days 30 --apply
#   scripts/clean-cargo-cache.sh --install-timer  # weekly systemd user timer
#   scripts/clean-cargo-cache.sh --uninstall-timer

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DAYS=14
APPLY=0
INSTALL_TIMER=0
UNINSTALL_TIMER=0
UNIT_NAME="lide-cargo-clean"

usage() {
    sed -n '2,30p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
    case "$1" in
        --apply) APPLY=1 ;;
        --days) shift; DAYS="${1:?--days needs a number}" ;;
        --install-timer) INSTALL_TIMER=1 ;;
        --uninstall-timer) UNINSTALL_TIMER=1 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done

case "${DAYS}" in
    ''|*[!0-9]*) echo "--days must be a whole number" >&2; exit 2 ;;
esac

target_dir() {
    if [ -n "${CARGO_TARGET_DIR:-}" ]; then
        printf '%s\n' "${CARGO_TARGET_DIR}"
        return
    fi
    local found
    found="$(cargo metadata --manifest-path "${ROOT}/Cargo.toml" --format-version 1 --no-deps 2>/dev/null \
        | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])' 2>/dev/null || true)"
    printf '%s\n' "${found:-${ROOT}/target}"
}

TARGET="$(target_dir)"
CARGO_HOME_DIR="${CARGO_HOME:-${HOME}/.cargo}"

if [ ! -d "${TARGET}" ]; then
    echo "no target directory at ${TARGET}" >&2
    exit 0
fi

# Do not delete artifacts under a running Cargo. Cargo locks <target>/.cargo-lock;
# take the same lock non-blockingly when flock is available.
if command -v flock >/dev/null 2>&1; then
    exec 9>"${TARGET}/.cargo-lock"
    if ! flock -n 9; then
        echo "cargo is building in ${TARGET}; try again when it is idle" >&2
        exit 0
    fi
fi

install_timer() {
    if ! command -v systemctl >/dev/null 2>&1; then
        echo "systemd is not available; run this script from cron instead:" >&2
        echo "  0 4 * * 0 ${ROOT}/scripts/clean-cargo-cache.sh --apply --days ${DAYS}" >&2
        exit 1
    fi
    local dir="${XDG_CONFIG_HOME:-${HOME}/.config}/systemd/user"
    local script
    script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
    mkdir -p "${dir}"
    cat > "${dir}/${UNIT_NAME}.service" <<SERVICE
[Unit]
Description=Remove stale Cargo build artifacts

[Service]
Type=oneshot
Nice=10
IOSchedulingClass=idle
ExecStart=${script} --apply --days ${DAYS}
SERVICE
    cat > "${dir}/${UNIT_NAME}.timer" <<TIMER
[Unit]
Description=Weekly Cargo cache cleanup

[Timer]
OnCalendar=weekly
Persistent=true
RandomizedDelaySec=1h

[Install]
WantedBy=timers.target
TIMER
    systemctl --user daemon-reload
    systemctl --user enable --now "${UNIT_NAME}.timer"
    echo "installed ${UNIT_NAME}.timer (weekly, --days ${DAYS})"
    echo "check it with: systemctl --user list-timers ${UNIT_NAME}.timer"
}

uninstall_timer() {
    local dir="${XDG_CONFIG_HOME:-${HOME}/.config}/systemd/user"
    systemctl --user disable --now "${UNIT_NAME}.timer" 2>/dev/null || true
    rm -f "${dir}/${UNIT_NAME}.service" "${dir}/${UNIT_NAME}.timer"
    systemctl --user daemon-reload 2>/dev/null || true
    echo "removed ${UNIT_NAME}.timer"
}

if [ "${INSTALL_TIMER}" = 1 ]; then
    install_timer
    exit 0
fi
if [ "${UNINSTALL_TIMER}" = 1 ]; then
    uninstall_timer
    exit 0
fi

# Collect the candidates. Null-delimited so paths with spaces survive.
candidates() {
    local profile
    # The Cargo lock is held, so no linker is running: temp files older than
    # an hour are leftovers from interrupted builds, not active output.
    find "${TARGET}" -type f -name '*.tmp*' -mmin +60 -print0

    # Native and cross-target profile directories.
    for profile in "${TARGET}"/*/build "${TARGET}"/*/*/build; do
        [ -d "${profile}" ] || continue
        find "${profile}" -mindepth 1 -maxdepth 1 -type d -mtime "+${DAYS}" -print0
    done
    for profile in "${TARGET}"/*/incremental "${TARGET}"/*/*/incremental; do
        [ -d "${profile}" ] || continue
        find "${profile}" -mindepth 1 -maxdepth 1 -type d -mtime "+${DAYS}" -print0
    done
    if [ -d "${CARGO_HOME_DIR}/registry/cache" ]; then
        find "${CARGO_HOME_DIR}/registry/cache" -type f -mtime "+${DAYS}" -print0
    fi
}

count=0
bytes=0
while IFS= read -r -d '' path; do
    count=$((count + 1))
    # `du -sk` is portable and cheap enough for the report.
    size="$(du -sk "${path}" 2>/dev/null | cut -f1 || echo 0)"
    bytes=$((bytes + size * 1024))
done < <(candidates)

human() {
    awk -v b="$1" 'BEGIN {
        split("B KiB MiB GiB TiB", units, " ");
        i = 1;
        while (b >= 1024 && i < 5) { b /= 1024; i++ }
        printf "%.1f %s", b, units[i]
    }'
}

echo "target:    ${TARGET}"
echo "cargo home: ${CARGO_HOME_DIR}"
echo "older than: ${DAYS} days"
echo "candidates: ${count} (about $(human "${bytes}"))"

if [ "${APPLY}" = 0 ]; then
    echo "report only; run with --apply to remove them"
    exit 0
fi

removed=0
while IFS= read -r -d '' path; do
    rm -rf -- "${path}"
    removed=$((removed + 1))
done < <(candidates)

echo "removed ${removed} items (about $(human "${bytes}"))"
