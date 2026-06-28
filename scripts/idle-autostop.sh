#!/usr/bin/env bash
# =============================================================================
# Generic idle auto-stop watchdog for a brev / cloud GPU instance.
#
# Powers the box OFF after it has been idle for IDLE_MIN consecutive minutes so
# we stop paying for the GPU when nobody is using it. An OS power-off transitions
# a brev/nebius instance to STOPPED, which halts GPU billing.
#
# "Idle" requires ALL of:
#   * GPU utilization  < GPU_PCT     (default 5 %)
#   * 1-min CPU load    < CPU_LOAD    (default 0.6)
#   * disk read rate    < DISK_KBPS   (default 3000 KB/s)
# The CPU + disk checks stop us from powering off mid model-load, when the GPU
# can sit at 0 % for minutes while ~30 GB of weights stream from a slow disk.
#
# Usage:
#   ./idle-autostop.sh run             # run the watchdog in the foreground
#   sudo ./idle-autostop.sh install    # install + enable a systemd service
#   sudo ./idle-autostop.sh uninstall  # remove the service
#   ./idle-autostop.sh status          # one-shot reading of the live metrics
#
# Tunables (env): IDLE_MIN GPU_PCT CPU_LOAD DISK_KBPS CHECK_SEC
# =============================================================================
set -euo pipefail

IDLE_MIN="${IDLE_MIN:-15}"
GPU_PCT="${GPU_PCT:-5}"
CPU_LOAD="${CPU_LOAD:-0.6}"
DISK_KBPS="${DISK_KBPS:-3000}"
CHECK_SEC="${CHECK_SEC:-30}"
SERVICE_NAME="idle-autostop"

log() { echo "[idle-autostop] $(date '+%H:%M:%S') $*"; }

gpu_util() {
  if command -v nvidia-smi >/dev/null 2>&1; then
    nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader,nounits 2>/dev/null \
      | tr -d ' ' | sort -rn | head -1
  else
    echo 0
  fi
}

cpu_load1() { awk '{print $1}' /proc/loadavg; }

# Total sectors read across real block devices (512 bytes each).
disk_sectors_read() {
  awk '$3 ~ /^(sd|nvme|vd|xvd|md)[a-z0-9]*$/ {s+=$6} END{print s+0}' /proc/diskstats
}

# returns 0 (true) when a < b, for floating point values
flt_lt() { awk -v a="$1" -v b="$2" 'BEGIN{exit !(a+0 < b+0)}'; }

read_metrics() {
  local g l c0 c1 dkb
  g="$(gpu_util)"
  l="$(cpu_load1)"
  c0="$(disk_sectors_read)"
  sleep "$CHECK_SEC"
  c1="$(disk_sectors_read)"
  dkb=$(( (c1 - c0) * 512 / 1024 / CHECK_SEC ))
  echo "$g $l $dkb"
}

run_watchdog() {
  local need=$(( IDLE_MIN * 60 ))
  local streak=0
  log "armed: IDLE_MIN=${IDLE_MIN} GPU_PCT<${GPU_PCT} CPU_LOAD<${CPU_LOAD} DISK_KBPS<${DISK_KBPS} (check every ${CHECK_SEC}s)"
  while true; do
    read -r g l dkb < <(read_metrics)
    if [ "${g:-0}" -lt "$GPU_PCT" ] && flt_lt "${l:-0}" "$CPU_LOAD" && [ "${dkb:-0}" -lt "$DISK_KBPS" ]; then
      streak=$(( streak + CHECK_SEC ))
    else
      if [ "$streak" -ne 0 ]; then log "active (gpu=${g}% load=${l} disk=${dkb}KB/s) -> reset"; fi
      streak=0
    fi
    if [ "$streak" -ge "$need" ]; then
      log "idle ${streak}s >= ${need}s -> powering off (gpu=${g}% load=${l} disk=${dkb}KB/s)"
      sync
      sudo -n shutdown -h now "idle-autostop: ${IDLE_MIN} min idle" 2>/dev/null \
        || sudo -n poweroff 2>/dev/null \
        || systemctl poweroff 2>/dev/null \
        || { log "POWER OFF FAILED (need passwordless sudo) — retrying later"; sleep 120; streak=0; }
      return 0
    fi
  done
}

install_service() {
  if [ "$(id -u)" -ne 0 ]; then echo "run with sudo: sudo $0 install" >&2; exit 1; fi
  local self; self="$(readlink -f "$0")"
  cat >/etc/systemd/system/${SERVICE_NAME}.service <<EOF
[Unit]
Description=Idle auto-stop watchdog (powers off the GPU box when unused)
After=network.target

[Service]
Type=simple
Environment=IDLE_MIN=${IDLE_MIN}
Environment=GPU_PCT=${GPU_PCT}
Environment=CPU_LOAD=${CPU_LOAD}
Environment=DISK_KBPS=${DISK_KBPS}
Environment=CHECK_SEC=${CHECK_SEC}
ExecStart=/usr/bin/env bash ${self} run
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF
  systemctl daemon-reload
  systemctl enable --now ${SERVICE_NAME}.service
  log "installed + started ${SERVICE_NAME}.service (IDLE_MIN=${IDLE_MIN})"
  systemctl --no-pager --lines=3 status ${SERVICE_NAME}.service || true
}

uninstall_service() {
  if [ "$(id -u)" -ne 0 ]; then echo "run with sudo: sudo $0 uninstall" >&2; exit 1; fi
  systemctl disable --now ${SERVICE_NAME}.service 2>/dev/null || true
  rm -f /etc/systemd/system/${SERVICE_NAME}.service
  systemctl daemon-reload
  log "uninstalled ${SERVICE_NAME}.service"
}

case "${1:-run}" in
  run)       run_watchdog ;;
  install)   install_service ;;
  uninstall) uninstall_service ;;
  status)    read -r g l dkb < <(read_metrics); echo "gpu=${g}% load=${l} disk=${dkb}KB/s (idle if gpu<${GPU_PCT} & load<${CPU_LOAD} & disk<${DISK_KBPS})" ;;
  *)         echo "usage: $0 {run|install|uninstall|status}" >&2; exit 2 ;;
esac
