#!/bin/bash

# Docker Compose를 사용한 실행 스크립트

# 색상 정의
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 프로젝트 루트 디렉토리로 이동
cd "$(dirname "$0")" || exit 1

# .env.docker 파일 확인
if [ ! -f .env.docker ]; then
    echo -e "${RED}Error: .env.docker 파일을 찾을 수 없습니다.${NC}"
    echo "프로젝트 루트에 .env.docker 파일을 생성해주세요."
    exit 1
fi

echo -e "${GREEN}.env.docker 파일을 사용합니다${NC}"

# Docker Compose로 서비스 시작
echo -e "${GREEN}Docker Compose로 서비스 시작 중...${NC}"
docker-compose up -d --build

if [ $? -ne 0 ]; then
    echo -e "${RED}Docker Compose 실행 실패${NC}"
    exit 1
fi

echo -e "${GREEN}서비스가 성공적으로 시작되었습니다!${NC}"
echo ""
echo "서비스 상태 확인: docker-compose ps"
echo "로그 확인: docker-compose logs -f api-server"
echo "전체 로그: docker-compose logs -f"
echo "서비스 중지: docker-compose down"
echo "서비스 재시작: docker-compose restart"
echo ""
echo "API 서버: http://localhost:8000"
echo "Swagger UI: http://localhost:8000/swagger-ui"
echo "PostgreSQL: localhost:5432"
echo "Redis: localhost:6379"

# 잠시 대기 후 서비스 상태 확인
sleep 5
docker-compose ps

# API 서버 로그 미리보기
echo ""
echo -e "${YELLOW}=== API 서버 초기 로그 ====${NC}"
docker-compose logs api-server 2>&1 | tail -20

# Health check 수행
echo ""
echo -e "${YELLOW}=== Health Check ====${NC}"
sleep 3
curl -f http://localhost:8000/health && echo -e "\n${GREEN}Health check 성공!${NC}" || echo -e "\n${RED}Health check 실패!${NC}"