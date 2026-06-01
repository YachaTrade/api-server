# feat/cms-dex-token-image

## Purpose

CMS(admin)용 dex_token 로고 이미지 등록 엔드포인트 `POST /cms/dex-token/image` 추가.
multipart로 token_id + image 받아 R2(`coin/{uuid}`)에 업로드하고 `dex_token.image_uri`에 저장.
기존 `/cms/token/metadata` 이미지 업로드 인프라 재사용. base: `v2`.

## Changes

- 설계 문서: `docs/superpowers/specs/2026-06-01-cms-dex-token-image-design.md`
- `POST /cms/dex-token/image` (admin, multipart) 추가 — token_id + image 받아 R2 `coin/{uuid}` 업로드 후 `dex_token.image_uri` 갱신
- token_id는 `valid_account_id`(순수 체크섬, 베니티 없음)로 정규화. 미존재 dex_token → 404 (업로드 前 존재확인으로 R2 orphan 방지)
- 기존 `validate_image`/`upload_new_image`/`R2Client::upload_metadata_image_file` 재사용
- TDD: 컨트롤러 `dex_token_exists`/`set_dex_token_image` 4 케이스 `#[sqlx::test]`
- 변경: `types/cms/mod.rs`, `controllers/cms/mod.rs`, `services/cms/mod.rs`, `router/cms/{path,mod,handler}.rs`, `main.rs`(openapi)

## Outcome

(PR/머지 시 작성)
