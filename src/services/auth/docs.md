## AuthService 개요

AuthService 는 지갑 서명 기반 인증 흐름을 담당한다. 세션 생성/삭제와 nonce 발급을 관리하며, Redis 와 Postgres 를 동시에 사용해 상태를 유지한다.

### 주요 기능
- **`generate_nonce`**: 지갑 주소 형식을 검증한 후 서명용 메시지를 생성하고 Redis 에 캐시한다. 도메인, 체인 ID, 발급 시각 정보를 메시지에 포함한다.
- **`create_session`**: 체인 ID 를 검증하고 서명에서 주소를 복구한다. 기존 계정을 조회하거나 없으면 생성하고, Redis·Postgres 에 세션을 기록한다. nonce 검증 실패 시 인증 오류를 반환한다.
- **`delete_session`**: Redis 세션 키와 Postgres 세션 레코드를 동시에 삭제한다.

### 내부 도우미
- **`verify_wallet`**: 서명과 메시지를 이용해 지갑 주소를 복구한다.
- **`generate_session_id`**: 주소·타임스탬프·UUID·메시지를 조합해 Base64 기반 세션 ID 를 만든다.

### 의존성
- `PostgresDatabase`, `RedisDatabase`
- `AccountController`: 계정 생성/조회
- `SessionController`: 세션 영속화
- 환경 변수 `APP_DOMAIN`, `CHAIN_ID`
