# 멀티스테이지 빌드를 위한 Dockerfile
# Stage 1: 빌드 스테이지
FROM --platform=linux/amd64 rust:1.89-slim AS builder

# 빌드 의존성 설치

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# 작업 디렉토리 설정
WORKDIR /app

# 의존성 캐싱을 위해 Cargo.toml과 Cargo.lock 먼저 복사
COPY Cargo.toml Cargo.lock ./

# 빈 src 디렉토리 생성하여 의존성만 먼저 빌드
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# 실제 소스 코드 복사
COPY . .

# sqlx 오프라인 모드 활성화 및 빌드 시 DATABASE_URL 설정
ENV SQLX_OFFLINE=true
ENV DATABASE_URL="postgres://user:password@localhost:5432/db"

# 릴리즈 빌드
RUN cargo build --release

# Stage 2: 런타임 스테이지
FROM --platform=linux/amd64 rust:1.89-slim

# 런타임 의존성 설치
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# 비 root 사용자 생성
RUN useradd -m -u 1001 -s /bin/bash appuser

# 작업 디렉토리 설정
WORKDIR /app

# 빌드된 바이너리 복사
COPY --from=builder /app/target/release/api-server /app/api-server

# Copy migrations
COPY migrations /app/migrations

# 실행 권한 부여
RUN chmod +x /app/api-server

# 소유권 변경
RUN chown -R appuser:appuser /app

# 사용자 전환
USER appuser

# 헬스체크 설정 (포트 8000으로 수정)
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1

# 포트 노출
EXPOSE 8000 8443

# 환경변수 설정 (기본값)
ENV RUST_LOG=info
ENV RUST_BACKTRACE=1

# 애플리케이션 실행
CMD ["./api-server"]