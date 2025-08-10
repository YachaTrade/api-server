#!/bin/bash

# 로컬 개발용 Docker 실행 스크립트
# 이미 빌드된 바이너리를 사용하여 빠르게 실행

# 색상 정의
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 프로젝트 루트 디렉토리로 이동
cd "$(dirname "$0")" || exit 1

# 바이너리가 있는지 확인
if [ ! -f "target/release/api-server" ]; then
    echo -e "${YELLOW}바이너리가 없습니다. 먼저 빌드합니다...${NC}"
    cargo build --release
    if [ $? -ne 0 ]; then
        echo -e "${RED}빌드 실패${NC}"
        exit 1
    fi
fi

# Docker Compose로 데이터베이스만 실행
echo -e "${GREEN}데이터베이스 서비스 시작 중...${NC}"
docker-compose up -d postgres redis

# 잠시 대기
echo -e "${YELLOW}데이터베이스 초기화 대기 중...${NC}"
sleep 5

# 마이그레이션 실행 (호스트에서 직접)
echo -e "${GREEN}마이그레이션 실행 중...${NC}"
export DATABASE_URL="postgres://nadsadmin:nadsadmin@localhost:5432/pump"
sqlx migrate run

# 로컬에서 직접 실행
echo -e "${GREEN}API 서버 시작 중...${NC}"
export $(grep -v '^#' .env | xargs)
./target/release/api-server