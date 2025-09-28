## SocialService 개요

SocialService 는 팔로우/언팔로우, 팔로잉 여부 확인, 목록 조회를 제공하는 소셜 기능 서비스 레이어다. 모든 데이터 조작은 `FollowController` 에 위임한다.

### 주요 기능
- **팔로우 추가/삭제**: 팔로워와 팔로잉 주소를 받아 컨트롤러 호출 결과를 `UpdateFollowResponse` 로 변환한다. 비즈니스 오류는 `BadRequest` 로 변환한다.
- **팔로우 여부 확인**: 특정 사용자 쌍의 팔로우 상태를 반환한다.
- **목록 조회**: `PaginationParams` 를 받아 팔로워/팔로잉 리스트를 조회하고, DB 오류는 `InternalError` 로 변환한다.

### 의존성
- `PostgresDatabase`
- `FollowController`: SQL 및 비즈니스 로직
- `PaginationParams`: 목록 페이지네이션
