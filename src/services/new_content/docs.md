## NewContentService 개요

NewContentService 는 최신 매수·매도·토큰 생성 정보를 단일 엔드포인트로 제공한다. Redis 캐시에 결과를 저장해 반복 호출을 최적화한다.

### 처리 흐름
1. `get_new_content` 호출 시 Redis 에 저장된 최신 콘텐츠를 우선 조회한다.
2. 캐시 미스가 발생하면 `NewContentController` 를 통해 Postgres 에서 데이터를 가져온다.
3. 조회 결과를 Redis 에 다시 저장하고 응답한다. 캐시 저장이 실패해도 경고 로그만 남기고 처리는 계속된다.

### 의존성
- `PostgresDatabase`, `RedisDatabase`
- `NewContentController`: 실제 DB 조회 로직을 담당
- `NewContentResponse`: 응답 모델
