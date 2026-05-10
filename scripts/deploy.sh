#!/usr/bin/env bash
#
# v2 api-server systemd 서비스 + bash alias 설치 스크립트.
# 기존 v1 service를 그대로 두고 별도의 unit (default: apiserver-v2)을 등록한다.
#
# - 모든 런타임 설정은 ${WORK_DIR}/.env 가 단일 source.
# - systemd unit은 PORT를 명시하지 않는다 (.env 의 PORT가 그대로 적용된다).
#   websocket-server에서 Environment=PORT= 박았다가 .env 무시되는 함정을
#   재현하지 않기 위함.
# - flush_all 동작은 .env 의 REDIS_KEY_PREFIX / REDIS_FLUSH_ON_STARTUP 로 제어.
#
# 사용:
#   ./scripts/deploy.sh
#
# 환경변수로 override 가능:
#   SERVICE_NAME    기본 apiserver-v2
#   WORK_DIR        기본 /home/ubuntu/api-server-v2
#   ALIAS_PREFIX    기본 api2
#   BASHRC          기본 ~/.bashrc
#   BIN_NAME        기본 api-server (Cargo.toml [package].name 과 일치)
#
# 사전:
#   git clone -b v2 <repo> api-server-v2
#   cd api-server-v2
#   cp ../api-server/.env .env       # PORT, REDIS_KEY_PREFIX 등 v2용으로 수정
#   cargo build --release

set -euo pipefail

SERVICE_NAME="${SERVICE_NAME:-apiserver-v2}"
WORK_DIR="${WORK_DIR:-/home/ubuntu/api-server-v2}"
ALIAS_PREFIX="${ALIAS_PREFIX:-api2}"
BASHRC="${BASHRC:-$HOME/.bashrc}"
BIN_NAME="${BIN_NAME:-api-server}"

SERVICE_PATH="/etc/systemd/system/${SERVICE_NAME}.service"
BIN_PATH="${WORK_DIR}/target/release/${BIN_NAME}"

ALIAS_MARKER_BEGIN="# >>> ${SERVICE_NAME} aliases (deploy.sh) >>>"
ALIAS_MARKER_END="# <<< ${SERVICE_NAME} aliases <<<"

echo "[deploy] service:  $SERVICE_NAME"
echo "[deploy] workdir:  $WORK_DIR"
echo "[deploy] binary:   $BIN_PATH"
echo "[deploy] env file: ${WORK_DIR}/.env"
echo "[deploy] alias:    ${ALIAS_PREFIX}{start,stop,restart,log,status}"
echo

if [[ ! -f "${WORK_DIR}/.env" ]]; then
    echo "[deploy] WARN: ${WORK_DIR}/.env 가 없습니다. dotenv 로딩이 실패해서 서버가 panic 합니다."
fi
if [[ ! -x "$BIN_PATH" ]]; then
    echo "[deploy] WARN: $BIN_PATH 가 없거나 실행 권한이 없습니다. 'cargo build --release' 먼저 실행하세요."
fi

sudo tee "$SERVICE_PATH" > /dev/null << EOF
[Unit]
Description=API Service (${SERVICE_NAME})
After=network.target

[Service]
User=ubuntu
Group=ubuntu
WorkingDirectory=${WORK_DIR}
ExecStart=${BIN_PATH}
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable "${SERVICE_NAME}.service"

touch "$BASHRC"
if grep -qF "$ALIAS_MARKER_BEGIN" "$BASHRC"; then
    sed -i.bak "/^${ALIAS_MARKER_BEGIN}\$/,/^${ALIAS_MARKER_END}\$/d" "$BASHRC"
fi

cat << EOF >> "$BASHRC"
${ALIAS_MARKER_BEGIN}
alias ${ALIAS_PREFIX}restart='sudo systemctl restart ${SERVICE_NAME} && journalctl -u ${SERVICE_NAME} -f -o cat'
alias ${ALIAS_PREFIX}start='sudo systemctl start ${SERVICE_NAME}'
alias ${ALIAS_PREFIX}stop='sudo systemctl stop ${SERVICE_NAME}'
alias ${ALIAS_PREFIX}log='journalctl -u ${SERVICE_NAME} -f -o cat'
alias ${ALIAS_PREFIX}status='sudo systemctl status ${SERVICE_NAME}'
${ALIAS_MARKER_END}
EOF

echo
echo "[deploy] done."
echo "[deploy] next:"
echo "[deploy]   source $BASHRC"
echo "[deploy]   ${ALIAS_PREFIX}start"
echo "[deploy]   ${ALIAS_PREFIX}log"
