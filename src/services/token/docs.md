## Token 서비스 모듈 개요

`services/token` 모듈은 토큰 상세 정보, 메타데이터, 생성 목록, 정렬 데이터 제공을 담당하는 네 개의 서브 서비스로 구성된다. 모든 서비스는 컨트롤러 레이어를 통해 Postgres 를 조회하고, 필요한 경우 Redis 캐시를 사용한다.

### TokenCreatedService (`create.rs`)
- 특정 계정이 생성한 토큰 목록을 페이지네이션과 함께 제공한다.
- `RedisDatabase` 에 계정·페이지 조합으로 결과를 캐시하고, `TokenCreatedController` 를 통해 DB 를 조회한다.
- 컨트롤러 오류는 로깅 후 `InternalError` 로 변환한다.

### TokenService (`detail.rs`)
- 단일 토큰의 상세 정보를 반환한다.
- 캐시 히트 시 Redis 응답을 사용하고, 미스 시 `TokenController` 로부터 값을 얻는다.
- 토큰을 찾지 못하면 `AppError::NotFound` 로 매핑한다.

### TokenMetadataService (`metadata.rs`)
- 토큰 메타데이터를 조회한다.
- Redis 캐시를 활용하며, 캐시 저장 실패 시 경고 로그만 남긴다.
- `TokenMetadataController` 가 실제 메타데이터 쿼리를 수행한다.

### TokenOrderService (`order.rs`)
- 생성 시간, 시가총액, 최신 거래, 검증된 크리에이터 등 다양한 정렬 기준을 지원한다.
- 정렬 결과, King of the Hill 정보, 총 개수를 결합해 `OrderMessage` 를 구성한다.
- Redis 에 정렬 기준과 페이지네이션 조합으로 캐시한다.

### 공통 의존성
- `PostgresDatabase`, `RedisDatabase`
- 각 도메인별 컨트롤러 (`TokenCreatedController`, `TokenController`, `TokenMetadataController`, `OrderController`)
- `PaginationParams`: 목록 API 공통 파라미터
