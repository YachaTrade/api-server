## SearchService 개요

SearchService 는 토큰/계정 검색 요청을 처리하며 Redis 를 이용해 동일한 검색어와 페이지네이션 조합을 캐시한다.

### 주요 동작
- **`search`**: 사용자 입력 문자열과 페이지 정보(`PaginationParams`)를 받아 Redis 에서 캐시된 결과를 찾는다.
  - 캐시 히트 시 즉시 응답을 반환한다.
  - 캐시 미스 시 `SearchController` 로 실제 검색을 수행하고, 성공 결과를 Redis 에 저장한다.
- 잘못된 입력이나 컨트롤러 오류는 `AppError::InternalError` 로 변환해 라우터 계층으로 전달한다.

### 의존성
- `PostgresDatabase`, `RedisDatabase`
- `SearchController`: 실제 DB 검색 로직
- `PaginationParams`: 페이지네이션 파라미터 모델
- `SearchResponse`: 검색 결과 DTO
