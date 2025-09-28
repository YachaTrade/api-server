## AccountService 개요

AccountService 는 계정 정보와 외부 연동(X, 지갑)을 처리하는 비즈니스 레이어로, 모든 데이터 접근은 `AccountController`, `AccountXController`, `WalletController` 를 통해 수행된다. 세션 주소를 기반으로 요청을 검증하고, 잘못된 입력에 대해서는 `AppError`를 반환한다.

### 주요 기능
- **프로필 수정(`update_account`)**: 닉네임/소개/이미지 입력을 정제하고 형식을 검증한 뒤 업데이트한다. 허용되지 않는 이미지 URL, 금지된 닉네임 접두어(@, #)를 즉시 거부한다.
- **프로필 조회(`get_account`)**: 세션 주소로 계정 정보를 조회하고, 컨트롤러 오류를 사용자 친화적인 메시지로 변환한다.
- **X 계정 연동(`connect_x`, `disconnect_x`, `update_x`)**: 요청 페이로드의 유효성을 점검하고, 연동 상태에 따라 적절한 에러 코드(예: NotFound)를 리턴한다.
- **지갑 등록/조회(`register_wallet`, `get_wallet`)**: 지갑 주소를 등록하거나 현재 연동된 지갑을 조회한다.

### 예외 및 로깅 전략
- 잘못된 사용자 입력은 모두 `AppError::BadRequest`로 매핑한다.
- 외부 연동 실패나 DB 오류는 로그로 경고(`warn`)를 남기고 `AppError::InternalError` 또는 적절한 오류로 변환한다.

### 의존성
- `PostgresDatabase`: 계정, X, 지갑 정보를 영속화.
- `AccountController` / `AccountXController` / `WalletController`: 실제 DB 쿼리 담당.
