# V2 Salt Mining 변경사항

## 슬랙 공유용

```
[API] V2 Salt Mining 지원 추가

POST /token/salt 엔드포인트에 version 필드 추가했습니다.

• 기존 호출은 변경 없이 그대로 동작 (version 미전달 시 V1)
• V2 토큰 생성 시 body에 "version": 2 추가하면 V2 본딩커브로 salt 계산

요청 예시:
{
  "creator": "0x...",
  "name": "My Token",
  "symbol": "MTK",
  "metadata_uri": "https://...",
  "version": 2        ← 이거만 추가
}

남은 작업:
1. V2_BONDING_CURVE, V2_TOKEN_IMPLEMENT 주소 확정 → .env 반영
2. deploy 환경별 env 반영 (eu, ca, sg, dev)
3. 프론트 연동 — 토큰 생성 시 version: 2 전달
```

## 상세 변경 내역

### 1. Request Body에 `version` 필드 추가
- **파일**: `src/types/token/salt.rs`
- `MineSaltRequest`에 `version: u8` 추가 (기본값 1)
- validation: 1 또는 2만 허용
- 기존 클라이언트는 `version` 안 보내면 자동으로 V1

### 2. MiningConfig version 분기
- **파일**: `src/services/token/salt.rs`
- `MiningConfig::load(version)` — version 파라미터 받도록 변경
- version=1: `BONDING_CURVE` + `TOKEN_IMPLEMENT`
- version=2: `V2_BONDING_CURVE` + `V2_TOKEN_IMPLEMENT`

### 3. 환경변수 추가
- **파일**: `.env`, `.env.example`
- `V2_BONDING_CURVE` — V2 본딩커브 컨트랙트 주소
- `V2_TOKEN_IMPLEMENT` — V2 토큰 구현 컨트랙트 주소

## 남은 작업

### 1. V2 컨트랙트 주소 설정
- `.env`에 `V2_BONDING_CURVE`, `V2_TOKEN_IMPLEMENT` 실제 주소 채우기
- deploy 환경별 `.env` (eu, ca, sg, dev)에도 반영 필요

### 2. 프론트엔드 연동
- 토큰 생성 시 `version: 2` 파라미터 전달하도록 수정

### 3. 테스트
- V2 config로 salt mining 동작 확인
- 기존 V1 호출이 영향 없는지 확인 (하위호환)
