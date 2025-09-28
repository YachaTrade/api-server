## Trading 서비스 모듈 개요

`services/trading` 모듈은 차트, 시세, 포지션, 스왑 이력, 시장 지표 등 거래 관련 데이터를 제공하는 다중 서브 서비스 집합이다. Redis 캐시를 적극 활용하여 빈번한 조회 부하를 줄인다.

### ChartService (`chart.rs`)
- TradingView 호환 캔들 데이터를 조회한다.
- 요청 파라미터(`GetBarsRequest`)를 포함한 캐시 키로 Redis 를 조회하고, 미스 시 `ChartController` 를 호출한다.

### MarketService (`market.rs`)
- 토큰의 실시간 시장 지표(가격, 유동성 등)를 반환한다.
- `MarketController` 에 위임하며, 결과를 Redis 에 캐시한다.

### MetricsService (`metrics.rs`)
- 특정 토큰의 거래 지표(거래량, 가격 변화 등)를 단일 혹은 복수의 기간(TimeFrame)으로 조회한다.
- Redis 캐시 없이 Postgres 만 사용하며, 컨트롤러 오류를 `InternalError` 로 래핑한다.

### PositionService (`position.rs`)
- 토큰 보유자 목록과 계정 보유 토큰 목록을 페이지네이션과 함께 제공한다.
- Redis 에 토큰/계정·페이지 조합으로 캐시하고, `PositionController` 가 DB 조회를 담당한다.

### PriceService (`price.rs`)
- 단일 토큰의 현재 가격을 반환한다. 간단한 조회이므로 캐시 없이 Postgres 만 사용한다.

### SwapService (`swap_history.rs`)
- 토큰별 스왑 이력(`SwapQuery` 기반)과 계정별 포지션 스왑 이력을 제공한다.
- 토큰 기준 조회는 Redis 캐시를 사용하고, 계정별 조회는 컨트롤러에 바로 위임한다.

### 공통 의존성
- `PostgresDatabase`, `RedisDatabase`
- 도메인별 컨트롤러 (`ChartController`, `MarketController`, `MetricsController`, `PositionController`, `PriceController`, `SwapController`)
- `PaginationParams`, `SwapQuery`, `TimeFrame` 등 도메인 전용 요청 모델
