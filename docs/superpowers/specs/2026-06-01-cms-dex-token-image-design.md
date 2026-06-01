# CMS dex_token 이미지 등록 엔드포인트 설계

- 작성일: 2026-06-01
- 브랜치: `feat/cms-dex-token-image` (base: `v2`)
- 상태: 승인됨 (design approved)

## 목표

CMS(admin)에서 특정 dex_token의 로고 이미지를 업로드해 `dex_token.image_uri`에 등록한다.
기존 `/cms/token/metadata` 이미지 업로드 인프라(R2 + 매직바이트 검증)를 재사용한다.

dex_token은 pool 발견(PairCreated)으로 자동 적재되는 외부 토큰 superset이며, 자체 `image_uri` 컬럼을 가진다.
DEX 토큰 목록은 `COALESCE(token.image_uri, dex_token.image_uri, quote_token.image_uri)`로 이미지를 고른다.

## 엔드포인트

```
POST /cms/dex-token/image
```

- 인증: `authenticate_user` 미들웨어 + service 내 `verify_admin` (기존 `/cms/*`와 동일)
- 본문: `multipart/form-data`, `DefaultBodyLimit::max(5_000_000)` (5MB)

### multipart 필드

| 이름 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `token_id` | text | ✅ | dex_token 주소. `valid_account_id`로 EIP-55 체크섬 정규화 (베니티 없음) |
| `image` | file | ✅ | 로고 이미지. 비어있으면 400 |

### 응답 (`Serialize + ToSchema`)

```rust
struct DexTokenImageResponse {
    success:   bool,
    token_id:  String,  // 체크섬 정규화된 주소
    image_uri: String,  // https://storage.nadapp.net/coin/{uuid}
}
```

## 데이터 흐름

```
handler upload_dex_token_image (router/cms/handler.rs)
  multipart → token_id(text), image(bytes)   // token_id/image 없으면 400
  └─ CmsService::update_dex_token_image(session_address, token_id, image_bytes)
       1. valid_account_id(token_id)        → 체크섬. None → 400
          (⚠ valid_token_id 아님: dex 토큰은 베니티 접미사 7777 없음 — 메모리 reference_valid_token_id_vanity)
       2. controller.verify_admin(session)  → false → 401
       3. controller.dex_token_exists(token_id) → false → 404  (업로드 前, R2 orphan 방지)
       4. upload_new_image(image_bytes)     ← 재사용:
            validate_image (magic byte: jpeg/png/webp/svg, ≥4B)
            + r2.upload_metadata_image_file(uuid, bytes, fmt)
            → key `coin/{uuid}` → `https://storage.nadapp.net/coin/{uuid}`
       5. controller.set_dex_token_image(token_id, url)
  → { success: true, token_id, image_uri: url }
```

### 컨트롤러 추가 (controllers/cms/mod.rs)

```rust
// 존재 확인 (업로드 前 404 판정)
async fn dex_token_exists(&self, token_id: &str) -> Result<bool>
//   SELECT EXISTS(SELECT 1 FROM dex_token WHERE token_id = $1)

// image_uri 갱신 (rows_affected>0)
async fn set_dex_token_image(&self, token_id: &str, image_uri: &str) -> Result<bool>
//   UPDATE dex_token SET image_uri = $1 WHERE token_id = $2
```

## 변경 파일

| 파일 | 변경 |
|------|------|
| `src/types/cms/mod.rs` | `DexTokenImageResponse`(ToSchema) 추가 |
| `src/controllers/cms/mod.rs` | `dex_token_exists`, `set_dex_token_image` + 테스트 |
| `src/services/cms/mod.rs` | `update_dex_token_image()` (verify_admin → exists → upload_new_image 재사용 → set_image) |
| `src/router/cms/path.rs` | `CmsPath::UploadDexTokenImage` + `/cms/dex-token/image` |
| `src/router/cms/mod.rs` | route 등록 (post + 5MB body limit + auth layer) |
| `src/router/cms/handler.rs` | `upload_dex_token_image` 핸들러 + multipart 추출 + `#[utoipa::path]` |
| `src/main.rs` | openapi `paths`에 핸들러, `components`에 `DexTokenImageResponse`(+ multipart schema) |

## 재사용 (신규 인프라 없음)

- `CmsService::validate_image` (jpeg/png/webp/svg, magic byte, ≥4B; 5MB는 라우터 body limit)
- `CmsService::upload_new_image` (uuid 생성 + r2 업로드 → `coin/{uuid}` URL)
- `R2Client::upload_metadata_image_file` (key `coin/{uuid}`)
- Rekognition 없음 (기존 플로우에도 없음).

## 테스트 (TDD, 컨트롤러 co-located `#[sqlx::test(migrations = "./migrations-test")]`)

DB 레이어만 단위 테스트 (R2 업로드/멀티파트는 기존 metadata 플로우처럼 통합 플레이스, 미테스트).
모든 token 주소는 체크섬 주소로 일관 사용.

1. **dex_token_exists** — seed 시 `true`, 미seed `false`
2. **set_dex_token_image (존재)** — image_uri 갱신 후 read-back 일치, `true` 반환
3. **set_dex_token_image (미존재)** — `false` 반환 (rows_affected 0)

## 검증 명령

```bash
cargo test --lib cms::   # controllers::cms 테스트 포함
cargo build
```

## 결정 로그

- token 검증: `valid_account_id`(순수 EIP-55 체크섬). dex 토큰은 외부 발견 토큰이라 베니티 접미사 없음 → `valid_token_id` 사용 시 거부됨. 모든 SQL bind에 체크섬 주소.
- 범위: image만 (token_id + image). name/symbol/decimals 편집은 범위 외.
- 미존재 토큰: 404 (update-only). 업로드 前 `dex_token_exists`로 판정해 R2 orphan 방지.
- 이미지 저장: `coin/{uuid}` (기존 token 이미지와 동일 경로). 기존 image_uri는 덮어쓰기(이전 R2 객체는 기존 플로우와 동일하게 orphan).
- 모더레이션: 없음(기존 플로우 동일, 매직바이트 포맷 검증만).
- admin 인가/존재확인은 **primary(write pool)**에서 수행(codex P2): replica 복제 지연으로 (a) 회수된 admin 통과, (b) 방금 적재된 토큰 false 404 를 방지.
- timeout: `/cms/dex-token/image`를 `method_based_timeout` 업로드 예외에 추가(5MB 업로드가 POST 타임아웃에 안 걸리게, codex P2).
- **스코프 한계(사용자 결정)**: `dex_token.image_uri`만 갱신. DEX 읽기는 `COALESCE(token.image_uri, dex_token.image_uri, ...)`라 `token` row도 있는 토큰은 dex_token 이미지가 가려짐 → 그런 토큰은 `/cms/token/metadata`로 관리. 이 엔드포인트는 순수 DEX 토큰 대상. (codex P2-2는 이 스코프로 수용)
