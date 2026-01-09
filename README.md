# NADS Pump API 서버 사양서

## 개요

NADS Pump API 서버는 탈중앙화 거래 플랫폼을 위해 설계된 고성능 Rust 기반 백엔드입니다. Axum 프레임워크로 구축되어 토큰 거래, 소셜 기능, 시장 데이터 관리를 위한 강력한 API를 엔터프라이즈급 보안과 확장성과 함께 제공합니다.

## 목차

- [기술 스택](#기술-스택)
- [아키텍처](#아키텍처)
- [타입 시스템](#타입-시스템)
- [API 엔드포인트](#api-엔드포인트)
- [인증 및 보안](#인증-및-보안)

## 기술 스택

- **언어**: Rust (2021 Edition)
- **웹 프레임워크**: [Axum](https://github.com/tokio-rs/axum) 0.7.5
- **비동기 런타임**: [Tokio](https://tokio.rs/) 1.38.0
- **데이터베이스 ORM**: [SQLx](https://github.com/launchbadge/sqlx) 0.8.1 (컴파일 타임 쿼리 검증)
- **API 문서화**: [Utoipa](https://github.com/juhaku/utoipa) (OpenAPI/Swagger 생성)
- **블록체인 통합**: [Alloy](https://github.com/alloy-rs/alloy) 0.11.0 (Ethereum/EVM 상호작용)
- **캐싱**: Redis (이중 인스턴스 설계)
- **객체 스토리지**: AWS S3

## 아키텍처

### 프로젝트 구조

```
src/
├── main.rs              # 애플리케이션 진입점
├── lib.rs               # 라이브러리 내보내기
├── state.rs             # 애플리케이션 상태 (공유 리소스)
├── config.rs            # 설정 상수
├── middleware.rs        # 인증 미들웨어
├── cors.rs              # CORS 설정
├── result.rs            # 에러 타입 및 처리
├── utils.rs             # 유틸리티 함수
│
├── db/                  # 데이터 액세스 레이어
│   ├── postgres/        # PostgreSQL (읽기/쓰기 분리)
│   ├── redis/           # Redis 캐싱 (이중 인스턴스 설계)
│   └── aws/             # AWS S3 클라이언트 (객체 스토리지)
│
├── router/              # HTTP 라우팅 레이어 (14개 기능 모듈)
│   ├── auth/            # 인증 (SIWE)
│   ├── account/         # 사용자 계정 관리
│   ├── token/           # 토큰 작업
│   ├── trade/           # 거래 기능
│   ├── order/           # 주문 관리
│   ├── follow/          # 소셜 팔로우 기능
│   ├── profile/         # 사용자 프로필
│   ├── search/          # 검색 기능
│   ├── bot/             # 봇 작업
│   ├── hype/            # 트렌딩/하이프 토큰
│   └── management/      # 관리자/관리 작업
│
└── types/               # 타입 정의 및 도메인 모델
    ├── common/          # 공유 타입
    ├── auth/            # 인증 타입
    ├── account/         # 계정 관련 타입
    ├── token/           # 토큰 관련 타입
    ├── trading/         # 거래 타입
    └── social/          # 소셜 기능 타입
```

### 핵심 컴포넌트

#### 애플리케이션 상태 (`state.rs`)

```rust
pub struct AppState {
    pub postgres: Arc<PostgresDatabase>,
    pub session_redis: Arc<RedisDatabase>,
    pub trade_redis: Arc<RedisDatabase>,
    pub s3_client: Arc<S3Client>,
}
```

- `Arc`를 사용한 스레드 안전 공유 상태
- 시작 시 한 번 초기화
- 각 요청에 대해 복제 (참조 카운트)

#### 데이터베이스 아키텍처

**PostgreSQL 설정**:

- **읽기/쓰기 분리**: 프라이머리와 레플리카용 별도 연결 풀
- **연결 풀링**: 쓰기 풀 (최대 50), 읽기 풀 (최대 1000)
- **최적화**: 트랜잭션 풀링 모드를 위한 PgBouncer
- **쿼리 안전성**: 컴파일 타임 쿼리 검증을 위한 SQLx

**Redis 아키텍처**:

- **이중 Redis 설계**:
  - **세션 Redis**: 인증 및 세션 데이터 (24시간 TTL)
  - **거래 Redis**: 시장 데이터 및 거래 캐시 (밀리초 단위 TTL)
- **캐싱 전략**: 전략적 TTL을 사용한 다단계 캐싱

### 요청 처리 파이프라인

1. Axum 서버에서 요청 수신
2. 미들웨어 체인:
   - 쿠키 관리
   - CORS 검증
   - 타임아웃 (10초)
   - 인증 (보호된 라우트의 경우)
3. 적절한 핸들러로 라우팅
4. 비즈니스 로직 실행
5. 응답 직렬화

## 타입 시스템

### 핵심 도메인 모델

#### 계정 모델

```rust
pub struct Account {
    pub account_id: String,      // 이더리움 주소
    pub nickname: String,
    pub image_uri: String,
    pub bio: String,
    pub follower_count: i32,
    pub following_count: i32,
    pub mutual: Option<Mutual>,  // 계산된 상호 친구
}
```

#### 토큰 모델

```rust
pub struct TokenWithAccountInfo {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
    pub description: Option<String>,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
    pub website: Option<String>,
    pub is_graduated: bool,
    pub created_at: i64,
    pub transaction_hash: String,
    pub account_info: AccountInfo,
    pub is_king: bool,
    pub is_king_created_at: Option<i64>,
    pub total_supply: BigDecimal,
    pub price: BigDecimal,
    pub market_cap: String,
}
```

#### 시장 모델

```rust
pub struct Market {
    pub market_type: String,     // "CURVE" 또는 "DEX"
    pub token_id: String,
    pub market_id: Option<String>,
    pub price: BigDecimal,
    pub latest_trade_at: i64,
    pub created_at: i64,
}
```

### 공통 패턴

#### 식별자 패턴 (`common/identifier.rs`)

이더리움 주소와 닉네임을 자동으로 감지하는 스마트 열거형:

```rust
pub enum Identifier {
    Nickname(String),
    Address(String),
}
```

#### 페이지네이션 패턴 (`common/pagination.rs`)

검증이 포함된 표준화된 페이지네이션:

```rust
pub struct PaginationParams {
    pub page: i64,        // 역순을 위한 음수 지원
    pub limit: i64,       // 최대 100, 검증됨
    pub direction: String, // "ASC" 또는 "DESC"
}
```

### 검증

시스템은 다층 검증을 구현합니다:

1. **역직렬화 검증**: 필드 검증을 위한 커스텀 역직렬화기
2. **구조체 수준 검증**: 요청 구조체의 `validate()` 같은 메서드
3. **타입 안전 파싱**: 정확성을 위한 Rust 타입 시스템 사용
4. **금융용 BigDecimal**: 모든 금전 값은 정밀도를 위해 `BigDecimal` 사용

### 에러 처리

```rust
pub enum AppError {
    AnyhowError(anyhow::Error),
    RouteError(String),
    RedisError(String),
    Unauthorized(String),
    AuthError(String),
    BadRequest(String),
    NotFound(String),
    InternalError(String),
    Conflict,
}

pub type AppResult<T> = Result<T, AppError>;
pub type AppJsonResult<T> = AppResult<Json<T>>;
```

## API 엔드포인트

### 인증 모듈 (`/auth/*`)

| 메서드 | 경로                   | 설명           | 인증 필요 |
| ------ | ---------------------- | -------------- | --------- |
| POST   | `/auth/nonce`          | 인증 논스 생성 | ❌        |
| POST   | `/auth/session`        | 인증 세션 생성 | ❌        |
| DELETE | `/auth/delete_session` | 인증 세션 삭제 | ✅        |

### 계정 모듈 (`/account/*`)

모든 엔드포인트는 인증이 필요합니다.

| 메서드 | 경로                       | 설명                    |
| ------ | -------------------------- | ----------------------- |
| PATCH  | `/account/update`          | 계정 프로필 업데이트    |
| GET    | `/account/get_account`     | 현재 계정 정보 가져오기 |
| PUT    | `/account/connect_x`       | X (트위터) 계정 연결    |
| DELETE | `/account/disconnect_x`    | X 계정 연결 해제        |
| GET    | `/account/x`               | 연결된 X 핸들 가져오기  |
| PATCH  | `/account/register_wallet` | 지갑 등록               |
| GET    | `/account/wallet`          | 등록된 지갑 가져오기    |

### 프로필 모듈 (`/profile/*`)

인증이 필요하지 않습니다.

| 메서드 | 경로                                  | 설명                        |
| ------ | ------------------------------------- | --------------------------- |
| GET    | `/profile/:account_id`                | 사용자 프로필 가져오기      |
| GET    | `/profile/hold-token/:account_id`     | 계정이 보유한 토큰 가져오기 |
| GET    | `/profile/tokens/created/:account_id` | 계정이 생성한 토큰 가져오기 |
| GET    | `/profile/swap-history/:account_id`   | 계정의 스왑 기록 가져오기   |

### 토큰 모듈 (`/token/*`)

인증이 필요하지 않습니다.

| 메서드 | 경로                     | 설명                     |
| ------ | ------------------------ | ------------------------ |
| GET    | `/token/:token`          | 토큰 정보 가져오기       |
| GET    | `/token/metadata/:token` | 토큰 메타데이터 가져오기 |

### 거래 모듈 (`/trade/*`)

인증이 필요하지 않습니다.

| 메서드 | 경로                                  | 설명                      |
| ------ | ------------------------------------- | ------------------------- |
| GET    | `/trade/swap-history/:token_id`       | 토큰의 스왑 기록 가져오기 |
| GET    | `/trade/holder/:token_id`             | 토큰 보유자 가져오기      |
| GET    | `/trade/market/:token_id`             | 시장 데이터 가져오기      |
| GET    | `/trade/chart/:token_id`              | 차트 데이터 가져오기      |
| GET    | `/trade/price/:token_id`              | 가격 데이터 가져오기      |
| GET    | `/trade/management-history/:token_id` | 관리 기록 가져오기        |

### 주문 모듈 (`/order/*`)

인증이 필요하지 않습니다.

| 메서드 | 경로                   | 설명                                 |
| ------ | ---------------------- | ------------------------------------ |
| GET    | `/order/creation_time` | 생성 시간순으로 정렬된 토큰 가져오기 |
| GET    | `/order/market_cap`    | 시가총액순으로 정렬된 토큰 가져오기  |
| GET    | `/order/latest_trade`  | 최신 거래순으로 정렬된 토큰 가져오기 |

### 검색 모듈 (`/search/*`)

| 메서드 | 경로            | 설명           |
| ------ | --------------- | -------------- |
| GET    | `/search/:name` | 토큰/계정 검색 |

### 팔로우 모듈 (`/follow/*`)

| 메서드 | 경로                             | 설명             | 인증 필요 |
| ------ | -------------------------------- | ---------------- | --------- |
| PUT    | `/follow/add`                    | 팔로우 추가      | ✅        |
| DELETE | `/follow/remove`                 | 팔로우 제거      | ✅        |
| GET    | `/follow/check/:account_id`      | 팔로우 여부 확인 | ✅        |
| GET    | `/follow/followers/:account_id`  | 팔로워 가져오기  | ❌        |
| GET    | `/follow/followings/:account_id` | 팔로잉 가져오기  | ❌        |

### 하이프 모듈

| 메서드 | 경로           | 설명                 |
| ------ | -------------- | -------------------- |
| GET    | `/hype_token`  | 하이프 토큰 가져오기 |
| GET    | `/honor_token` | 명예 토큰 가져오기   |

### 봇 모듈 (`/bot/*`)

| 메서드 | 경로            | 설명                                       |
| ------ | --------------- | ------------------------------------------ |
| POST   | `/bot/metadata` | 봇의 메타데이터 설정 (multipart/form-data) |

### 관리 모듈 (`/management/*`)

모든 엔드포인트는 인증이 필요합니다.

| 메서드 | 경로                     | 설명                      |
| ------ | ------------------------ | ------------------------- |
| GET    | `/management/dev`        | 개발자 포지션 가져오기    |
| GET    | `/management/hold_token` | 보유 토큰 관리 가져오기   |
| GET    | `/management/lock`       | 계정 잠금 가져오기        |
| GET    | `/management/withdraw`   | 출금 가능한 잠금 가져오기 |

### API 기능

- **Swagger UI**: `/dev-sw`에서 사용 가능
- **헬스 체크**: `/health`에서 사용 가능
- **페이지네이션**: `PaginationParams` 쿼리 파라미터를 통해 지원
- **파일 업로드**: 봇 모듈에서 지원 (multipart/form-data)
- **응답 형식**: 모든 응답은 JSON

## 인증 및 보안

### 이더리움으로 로그인 (SIWE)

서버는 EIP-4361 호환 인증을 구현합니다:

1. **논스 생성**:

   - 클라이언트가 이더리움 주소로 논스 요청
   - 서버가 UUID 기반 논스 생성
   - EIP-4361 형식의 메시지 반환

2. **세션 생성**:
   - 클라이언트가 지갑으로 메시지 서명
   - 서버가 서명 검증
   - 세션 ID 생성: `base64(address-timestamp-uuid-message)[0:32]`
   - Redis와 PostgreSQL에 세션 저장
   - HTTP-only 보안 쿠키 반환

### 세션 관리

- **이중 저장소**: Redis (캐시) + PostgreSQL (영속성)
- **쿠키 설정**:
  - `HttpOnly`: true
  - `Secure`: true
  - `SameSite`: None
  - `Max-Age`: 7일

### 보안 기능

#### CORS 설정

- **프로덕션 오리진**: `https://nad.fun`, `https://nadapp.net`
- **개발 환경**: 환경 변수를 통해 설정 가능
- **메서드**: GET, PUT, POST, PATCH, DELETE, OPTIONS
- **자격 증명**: 활성화

#### SQL 인젝션 방지

- 모든 쿼리는 SQLx 매개변수화된 구문 사용
- 컴파일 타임 SQL 검증
- 쿼리에 문자열 연결 사용 안 함

#### 입력 검증

- 역직렬화 수준 검증
- 민감한 필드에 대한 커스텀 검증기
- Rust 타입 시스템을 사용한 타입 안전 파싱

#### 속도 제한 (설정 가능)

- 초당 100 요청
- 버스트 크기 10
- IP 기반 속도 제한

#### 추가 보안

- 요청 타임아웃: 10초
- 헬스 체크가 포함된 연결 풀링
- 구조화된 에러 처리
- 이더리움 secp256k1 서명 검증

### 보안 모범 사례

1. 모든 민감한 작업은 인증 필요
2. 세션 데이터는 응답에 절대 노출되지 않음
3. 암호화 작업은 표준 라이브러리 사용
4. 다층 입력 검증
5. 모든 데이터베이스 쿼리에 준비된 구문 사용
6. 보안 쿠키 처리
7. 프로덕션 도메인에 맞게 CORS 적절히 설정

## 서버 실행

### 개발 환경

```bash
cargo run -- --port 8080
```

### 프로덕션 환경

```bash
cargo build --release
./target/release/api-server
```

### 환경 변수

- `APP_DOMAIN`: SIWE를 위한 애플리케이션 도메인
- `CHAIN_ID`: 이더리움 체인 ID
- `DATABASE_URL`: PostgreSQL 연결 문자열
- `DATABASE_REPLICA_URL`: PostgreSQL 레플리카 연결 문자열
- `REDIS_URL`: Redis 연결 문자열
- `AWS_ACCESS_KEY_ID`: AWS 자격 증명
- `AWS_SECRET_ACCESS_KEY`: AWS 자격 증명
- `ALLOW_CORS_PORT`: 개발 환경 CORS 포트
- `CORS_ALLOWED_ORIGINS`: 추가 CORS 오리진

## 성능 최적화

- **Async/Await**: 논블로킹 I/O를 위한 완전한 Tokio 런타임
- **연결 풀링**: 높은 동시성을 위해 최적화됨
- **캐싱 전략**: Redis를 사용한 다단계 캐싱
- **배치 작업**: 데이터베이스 왕복 횟수 감소
- **컴파일 타임 최적화**: 컴파일 시 쿼리 검증
- **읽기 레플리카**: 확장을 위한 별도의 읽기 풀

## 모니터링 및 관찰성

- `/health`의 헬스 엔드포인트
- 전체적인 구조화된 로깅
- 상세한 컨텍스트가 포함된 에러 추적
- 연결 풀을 통한 성능 메트릭

---

버그 리포트 및 기능 요청은 [GitHub Issues](https://github.com/anthropics/claude-code/issues) 페이지를 방문해 주세요.
