# refactor/drop-v2-table-prefix

## Purpose

DB 식별자에서 `v2_` 접두어를 제거하고(GIWA는 단일 컨트랙트 세대), 테스트가 GIWA 배포 스키마 SSOT를 그대로 적용하도록 전환한다.

## Changes

- SQL 문자열의 테이블명 25종에서 `v2_` 제거. **컬럼 alias·Redis 키·로컬 변수의 `v2_`는 테이블이 아니므로 보존**(예: `v2_current_balance`, `v2_total_claimed`)
- `migrations` 서브모듈을 `Naddotfun/migrations` → `YachaTrade/migrations`로 교체, `migrations-test/`(레거시 사본) 삭제 후 `sqlx::test(migrations=...)` 191곳을 `./migrations`로 전환
- 테스트 픽스처 정합: quote 주소를 Monad MON → GIWA WETH predeploy로, 스키마가 시드하는 WETH 행을 반영해 count/충돌 처리 조정
- 전제: `giwa/migrations`가 `v2_` 제거 + x_verification/dev_post 스키마 추가 + dev_post id를 일반 시퀀스로 전환
- 검증: build + lib 321 passed / 4 failed. 실패 4건은 dev_post 하니스 문제로 **작업 착수 전 베이스라인에서도 동일 실패**

## Outcome

- (머지 시 작성)

## 배포 의존성

observer · api-server · giwa/migrations 세 리포가 **함께** 배포돼야 한다. 부분 롤아웃 시 해당 테이블 대상 쿼리가 전부 실패한다.
