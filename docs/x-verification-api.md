# X Verification API 문서 (X Hidden Creator Launch)

## 개요

X Verification API는 코인 창작자가 자신의 X(Twitter) 계정을 OAuth2로 검증하되, **실제 X 핸들/user_id는 절대 노출하지 않고** 다음 두 집계 신호만 코인에 공개로 붙이는 기능입니다.

- **Followers** — 창작자 X 계정의 팔로워 수
- **Followed by** — 창작자가 직접 등록한 X 핸들(최대 3개)이 실제로 창작자를 팔로우하는지 검증한 결과(각 핸들의 X 블루배지 여부 포함)

지갑 로그인(`docs/auth-api.md`)과는 **별개의 인증 메커니즘**입니다 — 지갑 세션은 "누가 요청하는가", X OAuth는 "그 지갑 창작자가 이 X 계정을 소유하는가"를 증명합니다. 전체 설계 배경과 의사결정 히스토리(특히 C1 치명적 결함과 수정)는 `docs/plans/2026-07-03-x-hidden-creator-verification-design.md` 참고. 이 문서는 **현재 머지된 최종 상태의 API 레퍼런스**입니다.

---

## 아키텍처 개요 — 두 인증 시스템의 관계

```
┌──────────────────────┐        ┌──────────────────────────┐
│  지갑 세션 (SIWE)      │        │  X OAuth2 PKCE            │
│  docs/auth-api.md     │        │  (이 문서)                 │
├──────────────────────┤        ├──────────────────────────┤
│ 목적: "누가 요청하는가" │        │ 목적: "이 창작자가 이 X    │
│                       │        │  계정을 실제로 소유하는가"  │
│ 저장: session_id       │        │ 저장: PKCE state/verifier, │
│  (Postgres+Redis)     │        │  X access_token (Redis만) │
│ 전달: HttpOnly 쿠키     │        │ 전달: 서버 내부에서만 소비, │
│  (SameSite=None)      │        │  클라이언트에 절대 노출 안됨 │
└──────────┬────────────┘        └──────────┬────────────────┘
           │                                │
           └──── X OAuth 라우트는 전부 ───────┘
                 지갑 세션이 있어야 시작 가능
                 (oauth/callback만 예외: X가 리다이렉트하는
                  공개 엔드포인트, state로 account_id 역추적)
```

핵심 특징: **X의 access_token/refresh_token은 Postgres에 절대 저장되지 않고, 클라이언트에도 절대 응답으로 나가지 않습니다.** Redis에 TTL로 짧게 살다가 `finalize` 직후 삭제되거나 TTL 만료로 자연 소멸합니다. 최종적으로 영구 저장되는 건 `followers_count`/`followed_by` 같은 **집계 결과**뿐입니다.

---

## 전체 플로우

```
1. [프론트] 지갑 로그인 (기존 세션, docs/auth-api.md)
2. [프론트] "Verify with X" 클릭
   → POST /x/oauth/login (세션 필요)
   → 서버: PKCE verifier/challenge 생성, state=랜덤 24바이트 hex
   → Redis: x_oauth:state:{state} = {account_id, code_verifier}  (TTL 10분)
   → 응답: { authorize_url }  ← 프론트는 이 URL로 리다이렉트
3. [X] 사용자가 X에서 인증
   → GET /x/oauth/callback?code&state  (공개, X가 리다이렉트, 세션/Origin 불필요)
   → 서버: Redis state GETDEL(1회성) → account_id 복원
   → 서버: code_verifier로 X 토큰 엔드포인트 code exchange
   → 서버: GET /2/users/me 로 followers_count 조회
   → Redis: x_pending:{account_id} = {x_user_id, access_token, followers_count, followed_by:[]}  (TTL 30분)
   → 프론트로 리다이렉트: 고정 서버 env URL (성공/실패만 구분, code/token 없음)
4. [프론트] followed_by 핸들 입력(최대 3개, 반복 가능)
   → POST /x/followed-by {handle}
   → 서버: X API로 팔로우 여부 확인 → 팔로우 중이면 pending.followed_by에 추가
5. [프론트] GET /x/verification/status → 현재 상태 렌더링
6. [프론트] Coin Detail 입력 완료 → 이미지/메타데이터 업로드 → POST /token/salt
   (docs/token-creation.md 참고, 여기서 token_id(CREATE2 주소)가 결정됨)
7. [프론트] salt 응답 직후, 아직 온체인 배포 전에 → POST /x/verification/reserve
   → 서버: creator==세션 확인 + CREATE2 재계산==token_id 확인
           + eth_getCode(token_id) 로 "아직 배포 안 됐음" 확인 (fail-closed)
           + first-writer-wins INSERT (token_id → account_id)
8. [프론트] (실제 온체인 토큰 생성 트랜잭션 실행은 이 API와 무관, 프론트가 지갑 서명으로 직접 호출)
9. [프론트] → POST /x/verification/finalize {token_id}
   → 서버: 예약 소유자==세션 확인 → pending 데이터를 token_id에 영구 저장(Postgres)
   → pending Redis 키 삭제
10. [누구나] GET /trade/xinfo/:token_id → 공개 조회 (미검증이면 x_verification: null)
```

---

## API 엔드포인트

### 1. OAuth 로그인 시작 (`POST /x/oauth/login`)

**인증**: 세션 필요 (지갑 로그인 쿠키)
**Rate limit**: 3 req/min per account (`X_OAUTH_LOGIN_RATE_LIMIT`)

#### 요청
Body 없음.

#### 응답
```json
{
  "authorize_url": "https://x.com/i/oauth2/authorize?response_type=code&client_id=...&redirect_uri=...&scope=users.read%20tweet.read%20offline.access&state=...&code_challenge=...&code_challenge_method=S256"
}
```

프론트는 이 URL로 브라우저를 리다이렉트(또는 팝업)하면 됩니다. `offline.access` scope로 `refresh_token`도 요청하지만 **서버가 실제로 저장/사용하지 않습니다** (아래 [토큰 라이프사이클](#토큰-라이프사이클) 참고).

#### 에러
- `429`: rate limit 초과

---

### 2. OAuth 로그아웃 (`POST /x/oauth/logout`)

**인증**: 세션 필요 (지갑 로그인 쿠키)

현재 세션의 X OAuth pending 로그인(Redis `x_pending`)을 삭제합니다. `finalize`가 성공 후 자동으로 지우는 것과 동일한 상태를, 검증을 끝내지 않고 취소/재시작하려는 사용자가 명시적으로 지울 수 있게 하는 엔드포인트입니다. 짧은 TTL(`X_PENDING_TTL_MS`) 만료로도 결국 사라지지만, 즉시 로그아웃/계정 전환 UX가 필요할 때 사용합니다.

#### 요청
Body 없음.

#### 응답
```json
{ "ok": true }
```

**멱등**: 로그인 상태가 아니어도(pending이 없어도) `200 { "ok": true }`를 반환합니다 — Redis `DEL`이 없는 키에 no-op이기 때문입니다.

> 참고: PKCE `state`(리다이렉트 핸드셰이크용 단기 값)는 여기서 건드리지 않습니다. 사용자가 인지하는 "로그인된" 상태는 `x_pending`뿐이며, `state`는 콜백에서 1회 소비되거나 TTL로 자동 만료됩니다.

---

### 3. OAuth 콜백 (`GET /x/oauth/callback`)

**인증**: 불필요 (X가 리다이렉트하는 공개 엔드포인트, Origin 헤더 검증도 안 함)

X가 `?code=...&state=...` (실패 시 `?error=...`)로 리다이렉트합니다. 서버는 **항상 고정된 서버 env URL로만** 브라우저를 리다이렉트합니다 — open-redirect 방지를 위해 클라이언트가 리다이렉트 대상을 지정할 방법이 없습니다.

| 결과 | 리다이렉트 대상 |
|------|----------------|
| 성공 | `X_OAUTH_REDIRECT_SUCCESS_URL` (쿼리 추가 없음) |
| X가 `error` 파라미터 반환 | `X_OAUTH_REDIRECT_FAILURE_URL?x_verify_error=denied` |
| `code`/`state` 누락 | `...&x_verify_error=bad_request` |
| state 만료/이미 소비됨 | `...&x_verify_error=state_expired` |
| 그 외 X API 실패 | `...&x_verify_error=x_error` |

프론트는 `X_OAUTH_REDIRECT_FAILURE_URL` 페이지에서 `location.search`의 `x_verify_error`를 읽어 에러 배너를 띄우면 됩니다.

#### 프론트에서 성공/실패 구분하기

운영 환경에서는 보통 `X_OAUTH_REDIRECT_SUCCESS_URL`과 `X_OAUTH_REDIRECT_FAILURE_URL`을 **같은 페이지**(예: 코인 생성 페이지)로 설정합니다 — 성공 시엔 그 URL에 이미 담긴 쿼리(예: `?x_verified=1`)만 그대로 오고, 실패 시엔 서버가 `?x_verify_error=<code>`(또는 `&x_verify_error=<code>`, base URL에 이미 `?`가 있으면)를 붙여서 옵니다. 즉 **같은 라우트로 성공/실패 둘 다 리다이렉트되므로, 프론트는 반드시 쿼리 파라미터로 분기**해야 합니다.

X 로그인 화면까지 실제로 브라우저가 이동했다 돌아오는 흐름이라 리액트 state는 이미 날아간 상태입니다. 그래서 성공 신호(`x_verified=1`)를 받으면 그 자체엔 실제 데이터(팔로워 수 등)가 없으므로, 곧바로 `GET /x/verification/status`를 호출해서 서버에 저장된 실제 결과를 다시 받아와야 합니다.

```tsx
useEffect(() => {
  const params = new URLSearchParams(window.location.search);
  if (params.get('x_verified') === '1') {
    fetchVerificationStatus(); // GET /x/verification/status 호출해서 실제 데이터 채움
  } else if (params.has('x_verify_error')) {
    showErrorBanner(params.get('x_verify_error')); // denied / bad_request / state_expired / x_error
  }
  // 새로고침 시 같은 처리가 반복되지 않도록 쿼리스트링 제거
  window.history.replaceState({}, '', window.location.pathname);
}, []);
```

`test-harness/xverify.html`도 동일한 패턴(리다이렉트 후 쿼리 파싱 → 배너 표시)으로 구현돼 있습니다.

---

### 4. Followed-by 추가 (`POST /x/followed-by`)

**인증**: 세션 필요
**Rate limit**: 5 req/min per account (`X_FOLLOWED_BY_RATE_LIMIT`, X API 호출 전에 최대 개수 컷으로 먼저 절약)

#### 요청
```json
{ "handle": "elonmusk" }
```
- `handle`은 `@` 접두사 제거 후 1~15자, `[A-Za-z0-9_]`만 허용.

#### 응답
```json
{
  "is_following": true,
  "entry": {
    "x_handle": "elonmusk",
    "x_image_uri": "https://pbs.twimg.com/.../elonmusk_400x400.jpg",
    "x_followers_count": 200000000,
    "is_x_verified": true
  }
}
```
- 팔로우하지 않는 경우 `is_following: false`, `entry` 없음 (저장도 안 함).
- 같은 핸들을 다시 등록하면 대소문자 무시 dedup으로 기존 항목을 새 스냅샷으로 갱신.
- 이미 3개(`X_FOLLOWED_BY_MAX`, 기본값 3) 차 있으면 X API 호출 전에 `400 max_followed_by_reached`.
- 핸들이 창작자를 팔로우하고 있어도 팔로워 수가 `X_FOLLOWED_BY_MIN_FOLLOWERS`(기본값 1000) 미만이면 `400 insufficient_followers`로 거부되고 저장되지 않음.

> **팔로워 수 반올림**: 응답에 노출되는 모든 팔로워 수(`followers_count`, `x_followers_count`)는 저장 시점에 1,000 단위로 **내림** 처리됩니다 (0–999 → 0, 1000–1999 → 1000, …). `/x/followed-by`, `/x/verification/status`, 공개 xinfo(`/trade/xinfo/:token_id`) 응답 전부 동일하게 적용됩니다. 정확한 수치를 노출하지 않기 위한 의도적인 버킷화이며, `insufficient_followers` 판정 자체는 반올림 전 원본 값으로 이뤄집니다.

#### 에러
- `400`: `invalid_handle` | `max_followed_by_reached` | `insufficient_followers`
- `410`: `verification_expired` (pending TTL 만료 — 처음부터 다시 `/x/oauth/login`)
- `429`: rate limit 초과

---

### 5. Followed-by 제거 (`DELETE /x/followed-by/:handle`)

**인증**: 세션 필요

응답은 `StatusResponse`(아래) 전체 — 제거 후 최신 상태를 그대로 돌려줍니다.

---

### 6. Status 조회 (`GET /x/verification/status`)

**인증**: 세션 필요

폼 렌더링용 — 아직 확정(finalize) 안 된 현재 계정의 진행 상태.

```json
{
  "followers_count": 128000,
  "followed_by": [
    { "x_handle": "elonmusk", "x_image_uri": "...", "x_followers_count": 200000000, "is_x_verified": true }
  ]
}
```
- pending이 없으면(`GET /x/oauth/login`을 아직 안 했거나 TTL 만료) `followers_count: null, followed_by: []` — **404가 아니라 200**.
- 창작자 본인의 X 핸들/user_id는 이 응답에 절대 포함 안 됨.

---

### 7. 예약 (`POST /x/verification/reserve`)

**인증**: 세션 필요
**Rate limit**: 5 req/min per account (`X_RESERVE_RATE_LIMIT` — 매 호출이 on-chain RPC(`eth_getCode`)를 하므로 낮게 설정)

`/token/salt` 응답을 받은 **직후, 아직 온체인 배포 전**(=salt가 아직 비밀인 시점)에 호출해야 하는 보안 핵심 엔드포인트입니다.

#### 요청
```json
{
  "token_id": "0x...",
  "creator": "0x...",
  "salt": "0x...",
  "version": "V2"
}
```
- `version`은 `/token/salt` 요청 때와 동일한 값이어야 하며 **필수**(생략 시 400 — 조용히 V1로 오판하지 않도록 serde default 없음).

#### 처리
1. `creator == 세션 지갑주소` 확인 (checksum 비교, `LOWER()` 금지)
2. `CREATE2(version, salt) == token_id` 재계산 확인 (salt 소유 증명)
3. **`eth_getCode(token_id)`로 아직 배포 안 됐는지 확인** — RPC 에러는 "배포 안 됨"으로 가정하지 않고 fail-closed로 503 처리
4. First-writer-wins INSERT (`token_id → account_id`)

#### 응답
```json
{ "ok": true }
```

#### 에러
- `400`: 형식 오류 (invalid salt/token_id/creator/session address)
- `403`: `creator_mismatch`
- `409`: `already_deployed` (이미 온체인 배포됨 — salt가 공개됐을 수 있어 거부) | `token_already_reserved` (다른 계정이 먼저 예약)
- `503`: `onchain_check_unavailable` (RPC 실패, fail-closed)
- `429`: rate limit 초과

---

### 8. 확정 (`POST /x/verification/finalize`)

**인증**: 세션 필요

**Reservation-only**입니다 — CREATE2/creator/salt 재검증은 전부 `reserve`가 이미 끝냈으므로, finalize는 오직 "이 `token_id`의 예약자가 지금 세션과 같은가"만 확인합니다.

#### 요청
```json
{ "token_id": "0x..." }
```

#### 처리
1. `token_x_reservation`에서 `token_id`의 소유자 조회 → 없으면 `403 not_reserved`
2. 소유자 ≠ 현재 세션 → `403 not_reserved`
3. `x_pending:{account_id}` 존재 확인 → 없으면 `410 verification_expired`
4. Postgres에 `token_x_verification` + `token_x_followed_by` **upsert**(idempotent — 재-finalize 가능, 예약 row는 삭제 안 됨)
5. `x_pending:{account_id}` Redis 키 삭제

#### 응답
```json
{ "ok": true }
```

#### 에러
- `403`: `not_reserved`
- `410`: `verification_expired`

---

### 9. 공개 조회 (`GET /trade/xinfo/:token_id`)

**인증**: 불필요 (완전 공개)

`token` 테이블 존재 여부와 무관하게 `token_id`(EIP-55 checksum, `valid_account_id`)만으로 조회합니다 — 온체인 인덱싱이 아직 안 됐어도 동작하도록 의도적으로 설계됨.

```json
{
  "x_verification": {
    "followers_count": 128000,
    "followed_by": [
      { "x_handle": "elonmusk", "x_image_uri": "...", "x_followers_count": 200000000, "is_x_verified": true }
    ]
  }
}
```
미검증 코인은 `x_verification: null` (404 아님).

---

## 토큰 라이프사이클

| 데이터 | 저장 위치 | TTL/영속성 | 클라이언트 노출 |
|--------|-----------|-----------|-----------------|
| PKCE `code_verifier` + `state` | Redis `x_oauth:state:{state}` | 10분(`X_OAUTH_STATE_TTL_MS`), 1회 `GETDEL` | 안 됨 |
| X `access_token` | Redis `x_pending:{account_id}` | 30분(`X_PENDING_TTL_MS`), finalize 또는 `/x/oauth/logout` 후 즉시 삭제 | **절대 안 됨** — 서버 내부에서 `check_follows_me` 호출용으로만 재사용 |
| X `refresh_token` | **저장 안 함** | — | 응답 구조체(`XTokenResponse`)엔 있지만 즉시 폐기 — `offline.access` scope 요청이 사실상 낭비되고 있음 |
| X `x_user_id` | Postgres `token_x_verification.x_user_id` | 영구 | **절대 안 됨** — 내부 식별자, API 응답에 없음 |
| `followers_count`, `followed_by[]` | Postgres | 영구 | 공개(누구나 `/trade/xinfo`로 조회 가능) — 이게 이 기능이 노출하려는 유일한 정보 |
| 지갑 세션 `session_id` | Postgres + Redis(캐시) | 24시간, `HttpOnly` 쿠키 | 쿠키로만(JS 접근 불가) — `docs/auth-api.md` 참고 |

**설계 의도**: 코인마다 매번 재인증하는 MVP 범위라, X access_token을 장기 보관해서 재사용할 이유가 없습니다. 창작자가 새 코인을 또 만들면 `/x/oauth/login`부터 다시 시작합니다.

---

## 보안 모델 요약 (C1 치명적 결함과 수정)

최초 설계는 `finalize` 안에서 CREATE2 재계산 + creator 검증을 했으나, **`compute_create2_address`는 creator를 입력으로 쓰지 않기 때문에**(creator는 salt 마이닝 시작점 힌트일 뿐, 주소 유도엔 무관) 이 검증은 실제로 "진짜 창작자"가 아니라 "이 salt 값을 아는 사람"만 증명했습니다. 결정적으로 **salt는 토큰 배포 트랜잭션(`BondingCurveRouter.create` calldata)에 실려 온체인에 영구 공개되므로**, 배포 후에는 누구든 salt를 읽어 자기 세션으로 finalize를 호출해 남의 코인에 가짜 신호를 심을 수 있었습니다(`ON CONFLICT DO UPDATE`라 기존 값 덮어쓰기까지 가능).

**수정**: `reserve`를 salt가 **아직 비밀인 시점**(온체인 배포 전)에 호출하도록 만들고, `eth_getCode`로 "아직 배포 안 됐음"을 fail-closed로 확인한 뒤 first-writer-wins로 예약을 기록합니다. `finalize`는 이제 그 예약과 세션이 같은지만 확인하는 단순한 권한 체크입니다. 온체인 값(salt 등)을 "아는 사람만 통과"하는 인증 근거로 쓰면 안 된다는 게 이 사고의 핵심 교훈입니다 — tx로 브로드캐스트되는 순간 비밀이 아니게 됩니다.

전체 경위와 위협 모델 분석은 `docs/plans/2026-07-03-x-hidden-creator-verification-design.md` §7 참고.

---

## 환경변수

```env
# X Developer Portal에서 발급
X_CLIENT_ID=...
# Confidential(Web App) 클라이언트일 때만 설정. 비어있으면 Public(Native) 클라이언트로 동작(PKCE만, secret 없음).
X_CLIENT_SECRET=
# X Developer Portal에 등록된 Callback URI와 정확히 일치해야 함
X_REDIRECT_URI=https://api.example.com/x/oauth/callback

# 콜백 완료 후 브라우저를 보낼 고정 프론트엔드 URL (서버 제어, open-redirect 방지)
X_OAUTH_REDIRECT_SUCCESS_URL=https://nadapp.net/create?x_verified=1
X_OAUTH_REDIRECT_FAILURE_URL=https://nadapp.net/create

# 선택 (기본값 있음)
X_OAUTH_STATE_TTL_MS=600000     # PKCE state 수명, 기본 10분
X_PENDING_TTL_MS=1800000        # pending 검증 수명, 기본 30분
X_FOLLOWED_BY_MAX=3             # 계정당 최대 followed-by 핸들 수, 기본 3
X_FOLLOWED_BY_MIN_FOLLOWERS=1000 # followed-by 핸들의 최소 팔로워 수, 기본 1000 (미만이면 400 insufficient_followers)
```

---

## TypeScript Interfaces

```typescript
// POST /x/oauth/login
interface OAuthLoginResponse {
  authorize_url: string;
}

// POST /x/followed-by
interface FollowedByRequest {
  handle: string;  // '@' 접두사는 자동 제거됨, 1-15자 [A-Za-z0-9_]
}
interface XFollowedByEntry {
  x_handle: string;
  x_image_uri: string;
  x_followers_count: number;
  is_x_verified: boolean;  // X 블루배지 여부
}
interface FollowedByResponse {
  is_following: boolean;
  entry?: XFollowedByEntry;  // is_following === false면 없음
}

// GET /x/verification/status, DELETE /x/followed-by/:handle
interface StatusResponse {
  followers_count?: number;   // pending 없으면 undefined
  followed_by: XFollowedByEntry[];
}

// POST /x/verification/reserve
interface ReserveRequest {
  token_id: string;
  creator: string;
  salt: string;
  version: "V1" | "V2";  // 필수, /token/salt 요청과 동일해야 함
}
interface ReserveResponse {
  ok: boolean;
}

// POST /x/verification/finalize
interface FinalizeRequest {
  token_id: string;
}
interface FinalizeResponse {
  ok: boolean;
}

// GET /trade/xinfo/:token_id
interface TokenXVerification {
  followers_count: number;
  followed_by: XFollowedByEntry[];
}
interface XInfoResponse {
  x_verification: TokenXVerification | null;
}
```

---

## 프론트엔드 연동 가이드 — Next.js / NextAuth(Auth.js)

프론트가 Next.js 기반이라면 NextAuth를 도입하고 싶어질 수 있지만, **두 인증 메커니즘의 용도가 서로 다르므로 같은 방식으로 접근하면 안 됩니다.**

### 지갑 세션(SIWE) 쪽

이 API 서버는 이미 자체 세션(쿠키, `SameSite=None`, 24시간)을 발급하고 관리합니다(`docs/auth-api.md`). NextAuth를 세션의 **source of truth**로 쓰지 마세요 — NextAuth 자체 JWT/쿠키까지 얹으면 세션이 이중화되어 "NextAuth는 로그인됐는데 api-server 쿠키는 만료됨" 같은 불일치가 생깁니다.

권장 패턴:
- NextAuth `CredentialsProvider`의 `authorize()` 콜백 안에서 `/auth/nonce` → 지갑 `personal_sign` → `/auth/session`을 그대로 호출
- 실제 인증 상태(무엇이 유효한 세션인가)는 계속 api-server의 `HttpOnly` 쿠키가 담당
- NextAuth의 `useSession()`/`getServerSession()`은 프론트 UI 상태 표시(로그인 여부 렌더링) 용도로만 사용
- 지갑 서명 UX 자체는 wagmi/RainbowKit 같은 라이브러리의 SIWE 지원과 조합하는 게 일반적

### X OAuth 쪽

**NextAuth의 Twitter/X Provider로 옮기지 마세요.** 그 Provider는 "이 X 계정으로 로그인"하는 표준 identity-provider 패턴을 전제하는데, 이 기능은 완전히 다른 용도입니다:

- OAuth `state`가 이미 로그인된 지갑 `account_id`에 묶여 있음 (일반적인 로그인 OAuth엔 이런 결합이 없음)
- `access_token`은 서버 내부에서 X API 호출 1~2회에 즉시 소모되고 클라이언트엔 절대 노출되지 않음
- `reserve`/`finalize`의 보안 모델(위 [보안 모델 요약](#보안-모델-요약-c1-치명적-결함과-수정))이 "이 X OAuth 콜백이 api-server 자체에서 처리된다"는 전제 위에 세밀하게 짜여 있음

NextAuth Provider로 바꾸면 PKCE code exchange가 Next.js 서버(route handler)로 옮겨가고, 그 결과(X user id/token)를 다시 api-server에 별도로 전달해야 하는 아키텍처 변경이 필요합니다. `X_CLIENT_SECRET` 관리 주체도 옮겨야 하고, C1 취약점 수정으로 다듬어놓은 보안 모델을 처음부터 재검증해야 합니다.

권장: 지금처럼 api-server가 OAuth state/token을 전담하게 두고, 프론트는 다음만 하면 됩니다.
1. `POST /x/oauth/login` 호출 → 받은 `authorize_url`로 리다이렉트
2. `X_OAUTH_REDIRECT_SUCCESS_URL`/`FAILURE_URL` 페이지에서 쿼리 파싱해 결과 표시
3. 나머지(`followed-by`, `status`, `reserve`, `finalize`)는 이 문서의 REST 엔드포인트를 직접 호출

즉, X 로그인 부분에 한해서는 **"OAuth 프로바이더 추상화"보다 "이미 검증된 보안 경계를 그대로 유지"가 우선**입니다.
