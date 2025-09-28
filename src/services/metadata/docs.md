## MetadataService 개요

MetadataService 는 토큰 메타데이터와 이미지 업로드 흐름을 관리한다. Cloudflare R2 저장소(R2Client), Redis, Postgres 를 조합해 파일 업로드와 검증을 수행한다.

### 주요 기능
- **이미지 업로드(`upload_image`)**: R2 에 이미지를 업로드하고, NSFW 판정 결과를 Redis 에 캐시한다. 업로드 성공 후 이미지 URL 과 NSFW 여부를 응답한다.
- **메타데이터 검증(`validate_metadata_request`)**: 업로드된 이미지의 NSFW 상태를 Redis 에서 조회하고, 메타데이터 필드를 검증한다. 이미지가 사전에 업로드되지 않았다면 BadRequest 로 거부한다.
- **메타데이터 업로드(`upload_metadata`)**: UUID 로 파일명을 생성하고 R2 에 JSON 메타데이터를 업로드한 뒤, Postgres 에 메타데이터 참조를 저장한다.

### 의존성
- `R2Client`: 이미지/메타데이터 객체 저장소 연동
- `PostgresDatabase`, `RedisDatabase`
- `MetadataController`: 메타데이터 DB 영속화
- `TokenMetadata`: 유효성 검증 로직을 보유한 도메인 모델
