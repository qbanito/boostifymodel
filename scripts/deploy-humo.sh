#!/usr/bin/env bash
# =============================================================================
# Deploy / relaunch the HuMo-17B inference server on a brev GPU box, with idle
# auto-shutdown ALWAYS enabled (both the app-aware timer inside serve_humo.py
# and the generic GPU/CPU/disk watchdog). Run this from the Mac after the box
# has been (re)provisioned.
#
#   ./scripts/deploy-humo.sh [INSTANCE_NAME] [IDLE_MIN]
#
# Defaults: INSTANCE_NAME=boostify-wan  IDLE_MIN=15
#
# After it finishes, open the tunnel in a separate terminal:
#   brev port-forward INSTANCE_NAME -p 8000:8000
# =============================================================================
set -euo pipefail

INSTANCE="${1:-boostify-wan}"
IDLE_MIN="${2:-15}"
HERE="$(cd "$(dirname "$0")" && pwd)"
export PATH="$HOME/.local/bin:$PATH"

echo "==> refreshing brev ssh config"
brev refresh >/dev/null 2>&1 || true

if ! brev ls 2>/dev/null | grep -q "$INSTANCE"; then
  echo "!! instance '$INSTANCE' is not in 'brev ls'."
  echo "   Provision/start it first:  brev start $INSTANCE   (or create a new H100)"
  exit 1
fi

echo "==> ensuring '$INSTANCE' is running"
brev start "$INSTANCE" >/dev/null 2>&1 || true
brev refresh >/dev/null 2>&1 || true

echo "==> copying serve_humo.py + idle-autostop.sh"
scp -o ConnectTimeout=30 "$HERE/../src-tauri/humo/serve_humo.py" "$INSTANCE":/home/ubuntu/HuMo/serve_humo.py
scp -o ConnectTimeout=30 "$HERE/idle-autostop.sh" "$INSTANCE":/home/ubuntu/idle-autostop.sh

echo "==> installing generic idle watchdog (systemd) + relaunching HuMo server"
ssh -o ConnectTimeout=30 "$INSTANCE" "bash -s" <<EOF
set -e
chmod +x /home/ubuntu/idle-autostop.sh
# Generic GPU/CPU/disk idle watchdog as a systemd service.
sudo IDLE_MIN=${IDLE_MIN} /home/ubuntu/idle-autostop.sh install || true
# App-aware HuMo server with its own idle timer (HUMO_IDLE_SHUTDOWN_MIN).
sudo systemctl reset-failed humo-server 2>/dev/null || true
sudo systemctl stop humo-server 2>/dev/null || true
sleep 1
sudo systemd-run --uid=ubuntu --gid=ubuntu \
  -E HOME=/home/ubuntu -E HUMO_IDLE_SHUTDOWN_MIN=${IDLE_MIN} \
  --unit=humo-server --working-directory=/home/ubuntu/HuMo \
  /home/ubuntu/HuMo/.venv/bin/python /home/ubuntu/HuMo/serve_humo.py
sleep 4
echo "---- health ----"
curl -s --max-time 8 http://127.0.0.1:8000/health || echo "server not up yet"
echo
echo "---- idle watchdog ----"
systemctl --no-pager --lines=2 status idle-autostop.service 2>/dev/null | head -5 || true
EOF

echo
echo "==> done. Open the tunnel in another terminal:"
echo "      export PATH=\"\$HOME/.local/bin:\$PATH\""
echo "      brev port-forward $INSTANCE -p 8000:8000"
echo
echo "    The box now auto-stops after ${IDLE_MIN} min idle (no render + nobody watching)."
