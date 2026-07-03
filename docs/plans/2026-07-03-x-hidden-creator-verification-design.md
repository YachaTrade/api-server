# X Hidden Creator Launch 검증 기능 설계

- **날짜**: 2026-07-03
- **대상 화면**: Coin 생성 폼의 "Hidden Creator Launch (Optional)" 패널
- **목적**: 프론트에서 X OAuth로 창작자를 인증하되, 창작자의 실제 X 계정(핸들)은 절대 노출하지 않고 "팔로워 수"와 "특정 계정이 나를 팔로우하는지" 같은 집계 신호만 코인에 붙여 공개한다.

---

## 개정 이력 (2026-07-03 최종 리뷰 — C1 치명적 결함 수정)

> 이 문서의 **§7.1 최초 설계(finalize에서 CREATE2 재계산 + creator 검증)** 는
> Task 1~11 구현이 전부 끝난 뒤 진행한 **whole-branch 최종 리뷰에서
> 치명적 결함(C1)** 이 확인되어 폐기되었다. 원문은 이력 보존을 위해 §7.1에
> 그대로 남겨두되, 그 아래에 재발견 경위와 최종 수정안을 추가했다.
>
> **C1 요약**: `compute_create2_address(deployer, implementation, salt)` 는
> **creator를 전혀 입력으로 쓰지 않는다** (deployer/implementation은
> `MiningConfig::load(version)` 가 env에서 로드하는 버전당 고정값이고, creator는
> salt "마이닝 시작점"을 정하는 데만 쓰여 주소 유도에는 무관 — 이미
> [reference_token_salt_create2] 로 확인된 사실). 따라서 finalize의 두 검증
> (`creator==session`, `CREATE2(salt)==token_id`)은 실제로는 **"이 salt 값을
> 아는 사람"** 만 증명할 뿐, "이 토큰의 진짜 창작자"를 증명하지 못한다. creator는
> 공격자가 자기 세션 주소로 그냥 채워 넣으면 되므로 creator 검증은 무의미하다.
>
> 결정타는 **salt가 영원한 비밀이 아니라는 것**이다. 토큰이 실제 온체인
> 배포되면(`BondingCurveRouter.create` calldata) salt가 공개된다. 배포 후에는
> 누구든 체인에서 salt를 읽어, 자기 세션으로 finalize를 호출해 남의(혹은 이
> 기능을 아예 쓰지도 않은) 코인에 가짜 팔로워/followed_by 데이터를 심을 수
> 있고, `ON CONFLICT (token_id) DO UPDATE` 라 기존 검증을 덮어쓰기까지 된다.
>
> **최종 수정안(승인됨)**: 새 `POST /x/verification/reserve` 엔드포인트를
> 도입한다. 프론트가 `/token/salt` 응답을 받은 **직후(=아직 온체인 배포 전,
> salt가 아직 비밀인 시점)** 세션 필수로 호출해 `token_id → account_id` 선점
> 예약을 기록한다. 예약 시점에 **온체인 `eth_getCode(token_id)` 로 "이미
> 배포됐는지"를 확인** — 이미 코드가 존재하면(=이미 배포됨=salt가 공개됐을 수
> 있음) 예약을 거부한다(fail-closed). 이후 `finalize` 는 이 예약과 세션이
> 일치하는지만 확인한다. 상세: **§3-bis**(플로우), **§4**(token_x_reservation),
> **§5**(reserve 라우트/타입 + 단순화된 finalize), **§7.1**(재발견 및 최종
> 수정), **§7.5**(reserve 위협모델).

---

## 1. 배경

Coin Detail 입력 전 단계에 "Verify with X" 버튼이 있고, 검증되면 두 가지 신호를 보여준다.

- **Followers** — 창작자 X 계정의 팔로워 수
- **Followed by** — 창작자가 직접 등록한 X 핸들(예: `https://x.com/elonmusk`)이 실제로 자신을 팔로우하는지 X API로 검증한 결과. 최대 3개.

이 신호는 **해당 코인에 대해서만** 유효하다 — 창작자 계정 자체(지갑)의 공개 프로필에는 절대 남지 않는다. 기존 `account_x` 테이블(공개 프로필용 X 연동, self-report, 실제로는 라우터에 마운트되지 않은 죽은 코드)과는 성격이 다르므로 별도로 설계한다.

---

## 2. 기존 구조 조사 결과 (설계에 영향을 준 부분)

- **세션**: `session` = 지갑 서명 기반 로그인(SIWE 스타일, `account_session` 테이블 + Redis `session:{id}:id`). X OAuth 세션과는 무관. 기존 `authenticate_user` 미들웨어(Origin 헤더 검증 → Redis-first, Postgres-fallback 세션 조회 → `Extension<String>`으로 `account_id` 주입)를 그대로 재사용한다.
- **`account_x` 테이블**은 `account_id` PK만 있고 `x_handle`엔 unique 제약이 없다. `connect_x`/`disconnect_x` 핸들러는 라우터에 마운트되지 않은 죽은 라우트이고, 클라이언트가 보낸 값을 그대로 INSERT하는 self-report 방식(OAuth 검증 없음)이다. 이번 기능은 이 흐름을 재사용하지 않고 완전히 새로 만든다.
- **토큰 생성 흐름에 "생성 확정" 콜백이 없다**: api-server는 이미지 업로드 → 메타데이터 등록 → `/token/salt`(CREATE2 salt/주소 미리 계산)까지만 관여한다. 실제 컨트랙트 배포(`BondingCurveRouter.create`)는 프론트가 지갑 서명으로 직접 호출하고, `token` 테이블 row는 온체인 이벤트를 보는 별도 Observer 인덱서가 나중에 만든다. api-server에 tx hash를 받는 엔드포인트가 없다.
  → **결정: `/token/salt` 응답 직후를 "확정 시점"으로 삼는다.** salt 응답이 `token_id`(CREATE2 주소)를 결정적으로 계산해주기 때문에, 그 시점에 별도 finalize 호출로 pending 데이터를 `token_id`에 영구 저장한다. 실제 온체인 배포가 끝내 일어나지 않으면 row가 고아로 남는데, 이는 salt 마이닝 자체가 이미 갖고 있는 리스크와 동일해서 새로운 문제가 아니다.
- **Redis에 이미 TTL 임시 데이터 패턴이 갖춰져 있다**: `PSETEX`(세션), `GETDEL`(nonce 원자적 소비 — 재사용 공격 방지), `INCR`+`EXPIRE`(rate limit). 코인 생성 전 pending 데이터는 이 패턴을 그대로 재사용해 Redis에 TTL로 둔다. Postgres `auth_nonce`처럼 `expires_at` 컬럼 방식도 가능하지만, 정리 job이 없다는 단점이 있고 애초에 코인 생성을 포기하면 자동 소멸되는 게 자연스러워 Redis를 택한다.
- **외부 API 연동 패턴**: `src/services/pricing/defillama.rs`가 참고 골격 — `Lazy<reqwest::Client>`(5s timeout) 싱글톤 + 실패 시 캐시하지 않고 빈 값 반환.
- **`TokenInfo`/`AccountInfo` 타입**: `AccountInfo`(`account_id`/`nickname`/`bio`/`image_uri`)는 token/account/auth/chester/dividend/hype/leaderboard/search/trading/trend 등 10개 이상 도메인에서 재사용되는 범용 타입이라 여기에 X 검증 필드를 넣으면 안 된다(전혀 무관한 응답에도 새어나감). `TokenInfo`(`src/types/common/info.rs`)에 새 optional 필드로만 추가한다.
  - 참고: `TokenRow` 쿼리가 `account.follower_count`/`account.following_count` 컬럼을 이미 SELECT하고 있으나 이는 **과거에 있었다가 지금은 쓰이지 않는 무관한 기능**이라 이름 충돌 걱정은 없음(확인 완료).

---

## 3. 전체 플로우

```
1. [프론트] 지갑 로그인(기존 세션)
2. [프론트] "Verify with X" 클릭 → POST /x/oauth/login (세션 필요)
   → 서버가 PKCE state/verifier 생성, Redis에 저장(account_id 매핑), X 인가 URL 반환
3. [X] 사용자 인증 → GET /x/oauth/callback?code&state (공개, X가 리다이렉트)
   → 서버가 code exchange, GET /2/users/me로 followers_count 조회
   → Redis pending 상태 저장(x_pending:{account_id}), 프론트로 리다이렉트(성공/실패만 전달)
4. [프론트] followed_by 핸들 입력(최대 3개) → POST /x/followed-by {handle} (반복)
   → 서버가 connection_status로 팔로우 여부 확인 + 대상 프로필(이미지/팔로워수) 조회 → Redis pending에 추가
5. [프론트] GET /x/verification/pending → 현재 상태 렌더링(followers_count, followed_by[])
6. [프론트] Coin Detail 입력 완료 → 이미지/메타데이터 업로드 → POST /token/salt
7. [서버] salt 응답 직후 POST /x/verification/finalize
   {token_id, creator, name, symbol, metadata_uri, salt}  -- salt 요청에 썼던 원본 파라미터 그대로 재전달
   → 서버가 CREATE2 주소를 직접 재계산해 token_id 일치 + creator==session 지갑주소 검증 (§7.1)
   → 통과 시 Redis pending → Postgres 영구 저장, Redis pending 삭제
8. [프론트] 컨트랙트 직접 호출(BondingCurveRouter.create) — 기존과 동일, 변경 없음
9. [Observer] 온체인 이벤트로 token row 생성 (기존과 동일)
10. [공개] GET /token/:token 응답에 x_verification 필드로 노출 (창작자 실제 핸들은 절대 노출 안 됨)
```

---

## 3-bis. 개정된 플로우 (2026-07-03 C1 수정 — reserve 도입)

위 §3의 7번 단계(finalize에서 CREATE2+creator 검증)가 C1으로 폐기되고 다음으로 대체된다. **핵심 변경은 `/token/salt` 직후·배포 직전에 `reserve` 를 끼워 넣는 것**이다.

```
1. [프론트] 지갑 로그인(기존 세션)                                    -- 변경 없음
2~5. [X OAuth / followed-by 수집]                                      -- 변경 없음 (reserve와 병렬 진행 가능)
6. [프론트] Coin Detail 입력 완료 → 이미지/메타데이터 업로드 → POST /token/salt
   → {salt, address(=token_id)} 응답 (이 시점 salt는 오직 이 응답 수신자만 앎)

★ 6.5 [프론트] POST /x/verification/reserve  (NEW, 세션 필수)         -- 배포 전에 반드시 호출
   body: {token_id, creator, salt, version}   -- salt 요청에 쓴 값 그대로
   서버가:
     a) creator == 세션 지갑주소            (EIP-55 체크섬, LOWER 금지)
     b) CREATE2(version, salt) == token_id  (salt 소지 증명 = 방어심층화)
     c) eth_getCode(token_id) 가 비어있음   (★ 핵심 방어선: 아직 미배포 = salt 여전히 비밀)
        └ RPC 오류 시 fail-closed: "미배포 확인 불가"로 간주해 예약 거부 (503)
     d) token_x_reservation 에 first-writer-wins INSERT (token_id → account_id)
        └ 이미 다른 account가 선점 → 409, 같은 account 재시도 → 멱등 200
   통과 시 예약 성립. (이 시점 이후 salt가 온체인에 공개돼도, 그땐 코드가
   존재하므로 c)에서 아무도 새로 예약 못 함 → 공격 차단)

7. [프론트] POST /x/verification/finalize  (세션 필수) — 단순화됨
   body: {token_id}                          -- creator/salt/name/symbol/metadata_uri/version 전부 제거
   서버가:
     a) token_id의 예약을 조회 → 존재하고 reservation.account_id == 세션  (아니면 403)
     b) Redis pending 존재 확인                                          (없으면 410)
     c) Postgres 영구 저장 (checksum 정규화된 token_id/account_id), pending 삭제
        └ 예약 row는 유지 (재-finalize 멱등성 + 이후 재검증 권한의 근거)

8. [프론트] 컨트랙트 직접 호출(BondingCurveRouter.create)              -- 변경 없음
9. [Observer] 온체인 이벤트로 token row 생성                           -- 변경 없음
10. [공개] GET /token/:token 응답에 x_verification 로 노출              -- 변경 없음
```

**이 기능을 아예 쓰지 않은 창작자의 토큰**도 배포 후엔 코드가 존재하므로 아무도(공격자도) 새로 예약할 수 없다 — fail-closed(예약 안 된 토큰은 영원히 검증 불가, 이것이 올바른 동작). 정상 창작자만 "배포 전"이라는 좁은 창에서 예약을 세션에 귀속시킨다.

**시점(TOCTOU) 안전성**: (c)의 미배포 확인과 (d)의 INSERT 사이에 토큰이 배포되더라도 안전하다 — 예약은 "미배포였던 시점의 예약자"를 정확히 기록하며, 그 시점 salt는 비밀이었으므로 예약자는 정당한 창작자다. 공격자가 이 창에 끼어들려면 애초에 미배포 상태에서 token_id를 알아야 하는데 그건 정상 경로와 동일하다.

---

## 4. DB 스키마

새 마이그레이션 파일(예: `migrations/00XX_token_x_verification.sql`). `token` 테이블에 대한 FK는 걸지 않는다 — finalize 시점엔 `token` row가 아직 존재하지 않기 때문(Observer가 나중에 생성).

```sql
-- 창작자 본인 검증 결과 (팔로워 수)
CREATE TABLE IF NOT EXISTS token_x_verification (
    token_id VARCHAR(42) PRIMARY KEY,
    account_id VARCHAR(42) NOT NULL,
    x_user_id VARCHAR(32) NOT NULL,      -- X 내부 id, 절대 외부 노출 안 함
    followers_count BIGINT NOT NULL,
    verified_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_token_x_verification_account_id
    ON token_x_verification (account_id);

-- 창작자가 직접 등록한 "이 사람이 나를 팔로우함" 핸들 (최대 3개, 공개 노출용)
-- 팔로우 안 하는 것으로 확인된 핸들은 애초에 저장하지 않는다(프론트에 실패만 알림).
CREATE TABLE IF NOT EXISTS token_x_followed_by (
    token_id VARCHAR(42) NOT NULL
        REFERENCES token_x_verification(token_id) ON DELETE CASCADE,
    x_handle VARCHAR(16) NOT NULL,
    x_image_uri VARCHAR NOT NULL,
    x_followers_count BIGINT NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (token_id, x_handle)
);
```

**(개정 2026-07-03) 배포 전 선점 예약 테이블** — 새 마이그레이션 `migrations/0037_token_x_reservation.sql`(+ `v2_upgrade_token_x_reservation.sql`). `token`/`token_x_verification` 어디에도 FK를 걸지 않는다(예약은 그 둘보다 먼저 생긴다). 예약 시점에 `token_id → account_id` 를 원자적으로 1회 기록하고, 이후 소유권 변경은 없다(row는 불변 → first-writer-wins 근거).

```sql
-- 0037_token_x_reservation.sql
-- 온체인 배포 전(salt 비밀 상태)에 token_id를 세션 계정에 선점 예약한다.
-- finalize는 이 예약과 세션 일치만 확인한다 (design §3-bis, §7.1 최종 수정).
CREATE TABLE IF NOT EXISTS token_x_reservation (
    token_id VARCHAR(42) PRIMARY KEY,          -- CREATE2 주소, EIP-55 체크섬 형태로 저장
    account_id VARCHAR(42) NOT NULL,           -- 예약한 세션 지갑주소 (체크섬)
    reserved_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_token_x_reservation_account_id
    ON token_x_reservation (account_id);
```

**Redis 키 (pending, 확정 전까지):**
- `x_oauth:state:{state}` → `{account_id, code_verifier}`, TTL 10분, 1회성이라 `GETDEL`로 소비
- `x_pending:{account_id}` → `{x_user_id, access_token, followers_count, followed_by: [...]}`, TTL 30분(`PSETEX`)

`access_token`/`refresh_token`은 Redis에만 짧게 살고 Postgres엔 저장하지 않는다 — finalize 후 재사용할 이유가 없다(코인마다 재인증하는 MVP 범위).

---

## 5. 모듈 구조 & API

기존 `router/<domain>/{mod.rs,path.rs,handler.rs}` 3분할 패턴을 그대로 따른다.

```
src/router/x_verification/{mod.rs, path.rs, handler.rs}
src/services/x_verification/mod.rs      -- Redis pending 관리, X API 호출 오케스트레이션
src/services/x_oauth/client.rs          -- X API 클라이언트 (defillama.rs 패턴)
src/controllers/x_verification/mod.rs   -- Postgres CRUD
src/types/x_verification/mod.rs         -- 요청/응답 DTO
src/types/token/x_verification.rs       -- TokenInfo에 붙는 공개 응답 타입
```

**라우트:**
```rust
POST   /x/oauth/login              // 세션 필요. PKCE state 생성, X 인가 URL 반환
GET    /x/oauth/callback           // 공개(X가 리다이렉트). state로 account_id 역추적, code exchange
POST   /x/followed-by              // 세션 필요. body:{handle}. 최대 3개 초과시 400 (X API 호출 전 컷)
DELETE /x/followed-by/:handle      // 세션 필요. pending에서 제거
GET    /x/verification/pending     // 세션 필요. 현재 pending 상태 조회(폼 렌더링용)
POST   /x/verification/finalize    // 세션 필요. body:{token_id, creator, name, symbol, metadata_uri, salt}
                                    // → 서버가 CREATE2 재계산 + creator==session_address 검증 후 확정 (§7 참고)
                                    //    ⚠️ 최초 설계 — C1으로 폐기. 아래 (개정) 참조.
```

**(개정 2026-07-03) reserve 추가 + finalize 단순화:**

```rust
POST   /x/verification/reserve     // NEW, 세션 필요. body:{token_id, creator, salt, version}
                                    // → creator==session + CREATE2(version,salt)==token_id
                                    //   + eth_getCode(token_id) 비어있음(fail-closed)
                                    //   + first-writer-wins INSERT(token_id→account_id)
                                    // 오류: 400(형식) / 403(creator_mismatch)
                                    //      / 409(already_deployed | token_already_reserved)
                                    //      / 503(onchain_check_unavailable)  200(ok)
POST   /x/verification/finalize    // (개정) 세션 필요. body:{token_id} 만.
                                    // → 예약 존재 && reservation.account_id==세션 확인 후 확정.
                                    //   CREATE2/creator/salt/name/symbol/metadata_uri/version 전부 제거
                                    //   (그 방어는 이제 reserve가 담당 — 중복 제거).
                                    // 오류: 403(not_reserved | creator_mismatch) / 410(verification_expired) / 200
```

**(개정) reserve 요청/응답 타입 (`src/types/x_verification/mod.rs`):**

```rust
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ReserveRequest {
    pub token_id: String,
    pub creator: String,
    pub salt: String,
    pub version: TokenVersion,     // ⚠️ serde default 없음 — 필수. I1(version 기본 V1) 근본 해결.
}
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ReserveResponse { pub ok: bool }

// FinalizeRequest 는 { token_id: String } 하나로 축소 (version 필드 제거 → I1 소멸).
```

> **I1(version 기본값 버그) 해결 위치**: 최초 `FinalizeRequest.version` 은 `#[serde(default = "default_version")]` 로 V1 기본값이라, 활성 버전 V2 토큰은 finalize가 CREATE2를 V1 impl/deployer로 계산해 항상 403이 났다. 개정안에서 CREATE2 재계산이 **reserve로 이동**하고 `FinalizeRequest` 에서 `version` 이 **완전히 제거**되므로 finalize 쪽 I1은 소멸한다. reserve의 `ReserveRequest.version` 은 **serde default를 두지 않아 필수**로 만든다 — 누락 시 조용히 V1로 오판하지 않고 400(deserialize 실패)이 나므로, 신규 엔드포인트에 대해 프론트가 `/token/salt` 에 보낸 것과 같은 version을 명시하도록 강제해 I1을 근본 차단한다.

**X API 클라이언트 함수:**
```rust
async fn build_authorize_url(state: &str, code_challenge: &str) -> String
async fn exchange_code(code: &str, code_verifier: &str) -> Result<XTokenResponse>
async fn get_me(access_token: &str) -> Result<XUserInfo>
    // GET /2/users/me?user.fields=public_metrics
async fn check_follows_me(access_token: &str, target_handle: &str) -> Result<XFollowCheck>
    // GET /2/users/by/username/{h}?user.fields=connection_status,profile_image_url,public_metrics
```

**설정 추가 (`.env`/`config.rs`):** `X_CLIENT_ID`, `X_CLIENT_SECRET`(confidential client 여부에 따라 optional), `X_REDIRECT_URI`, `X_OAUTH_STATE_TTL_MS`, `X_PENDING_TTL_MS`.

---

## 6. 타입 설계 (`TokenInfo` 확장)

```rust
// src/types/common/info.rs — 기존 TokenInfo에 필드 추가
pub struct TokenInfo {
    // ...기존 필드...
    pub creator: AccountInfo,
    pub is_cto: bool,
    pub version: TokenVersion,
    pub x_verification: Option<TokenXVerification>,   // 신규
}

// src/types/token/x_verification.rs (신규)
pub struct TokenXVerification {
    pub followers_count: i64,
    pub followed_by: Vec<XFollowedByEntry>,
}
pub struct XFollowedByEntry {
    pub x_handle: String,
    pub x_image_uri: String,
    pub x_followers_count: i64,
}
```

검증 안 된 토큰은 `x_verification: None` → 응답에서 `null`. `AccountInfo`에는 어떤 필드도 추가하지 않는다(다른 10여개 도메인으로 새어나가는 것을 막기 위함).

**공개 응답 예시 (`GET /token/:token`):**
```json
{
  "token_id": "0x...",
  "name": "...",
  "x_verification": {
    "followers_count": 128000,
    "followed_by": [
      { "x_handle": "elonmusk", "x_image_uri": "https://...", "x_followers_count": 200000000 }
    ]
  }
}
```

---

## 7. 보안 리뷰 (security-review 스킬로 점검, 2026-07-03)

### 7.1 CRITICAL — `finalize` IDOR / creator 검증 누락

> ⚠️ **아래 "최초 설계"는 C1으로 폐기됨. 최종 수정안은 그 다음 "재발견 및 최종 수정" 소절 참조.** (이력 보존을 위해 원문 유지)

#### 7.1-A 최초 설계 (결함 있음 — 폐기)

**문제**: `/token/salt`는 인증이 전혀 없고(`TokenPath::Salt`에 `auth_layer` 미적용), 요청 바디의 `creator`도 클라이언트가 임의로 채우는 필드이며(`Extension<String>` session_address 미사용), 서버는 어떤 계정이 어떤 salt/token_id를 요청했는지 Postgres/Redis 어디에도 기록하지 않는다(`mine_salt`는 순수 계산 후 응답만 반환, `AppState`도 안 씀). salt 자체도 요청마다 랜덤 `uuid` 기반이라 재현 불가능하다.

이 상태에서 `finalize`가 단순히 `{token_id}`와 세션만으로 확정한다면, 로그인한 임의의 계정이 실제 소유/생성 여부와 무관하게 아무 `token_id`에나 자신의 (가짜 X 계정 포함) 검증 데이터를 선점해 붙일 수 있다 — 남의 코인에 조작된 소셜 시그널을 심는 것이 가능해진다.

**결정**: `finalize` 요청에 salt 채굴 시 사용한 원본 파라미터(`creator, name, symbol, metadata_uri, salt`)를 그대로 재전달받아, 서버가 `src/services/token/salt.rs`의 `compute_create2_address`를 재사용해 **CREATE2 주소를 직접 재계산**하고 다음 두 조건을 모두 검증한다.
1. 재계산한 주소 == 요청의 `token_id`
2. `creator`(EIP-55 체크섬 비교, `LOWER()` 금지) == 인증된 세션의 지갑 주소

이 salt 값을 실제로 알고, 그 지갑을 소유한 사람만 finalize를 통과할 수 있다. `/token/salt` 엔드포인트 자체는 건드리지 않는다(다른 용도로 비로그인 접근을 허용하는 것일 수 있어 이번 기능 범위 밖으로 판단).

#### 7.1-B 재발견 및 최종 수정 (2026-07-03 whole-branch 최종 리뷰)

**7.1-A가 왜 실패하는가 (C1)**: 위 "결정"의 전제는 *"이 salt 값을 아는 사람 == 진짜 창작자"* 였다. 이 전제가 두 가지 이유로 무너진다.

1. **creator 검증은 무의미하다.** `compute_create2_address(deployer, implementation, salt)` 는 creator를 입력으로 쓰지 않는다(주소는 deployer/implementation/salt로만 결정 — [reference_token_salt_create2]). finalize의 `creator==session` 은 요청 바디의 `creator` 를 세션 주소와 비교하는데, 공격자는 `creator` 를 자기 세션 주소로 채우면 그만이라 항상 통과한다. 즉 이 검증은 "token_id를 만든 사람"을 전혀 증명하지 못하고, 오직 남은 방어는 `CREATE2(salt)==token_id`("salt를 아는가") 하나뿐이다.
2. **salt는 영구 비밀이 아니다.** 토큰이 온체인 배포되면(`BondingCurveRouter.create` tx calldata) salt가 공개된다. 배포 후에는 **누구든** 체인에서 salt를 읽어 `CREATE2(salt)==token_id` 를 통과시킬 수 있고, `creator=자기세션` 으로 두면 finalize 전체를 통과한다. `ON CONFLICT (token_id) DO UPDATE` 라 **이 기능을 쓰지도 않은 남의 코인**에까지 가짜 팔로워/followed_by를 심거나 기존 검증을 덮어쓸 수 있다.

핵심 통찰: **정상 창작자와 공격자를 가르는 유일한 신호는 "시점(timing)" 이다.** 정상 창작자는 salt가 아직 비밀인 배포 전에 행동하고, 공격자는 salt가 공개된 배포 후에 행동한다. CREATE2 재계산은 이 시점을 구분하지 못한다.

**최종 결정 (승인됨) — reserve 도입 + finalize를 reservation-only로 단순화**:

1. **새 `POST /x/verification/reserve`** (세션 필수)를 `/token/salt` 응답 직후·배포 전에 호출한다. reserve가 다음을 모두 확인한다:
   - `creator == 세션 지갑` (EIP-55 체크섬) — 값싼 1차 필터.
   - `CREATE2(version, salt) == token_id` — **salt 소지 증명**(방어심층화). token_id만 유출되고 salt는 유출되지 않은 좁은 경우까지 막는다. 여기서 `version` 을 쓰므로 CREATE2를 활성 버전(V2)으로 정확히 계산 → **I1 근본 해결**.
   - **`eth_getCode(token_id)` 가 비어있음** — ★ 진짜 방어선. 코드가 있으면(=이미 배포=salt 공개 가능) 예약 거부. **RPC 오류 시 fail-closed**(미배포로 낙관 금지, 예약 거부 503).
   - `token_x_reservation` 에 **first-writer-wins** 원자적 INSERT. 다른 계정 선점 시 409, 같은 계정 재시도는 멱등.
2. **`finalize` 는 `{token_id}` 만 받아** 예약 존재 + `reservation.account_id == 세션` 만 확인하고 확정한다.

**왜 finalize에서 CREATE2 재검증을 유지하지 않고 제거하는가 (설계 판단 + 근거)**:

- **비중복성**: reserve가 이미 (배포 전에) CREATE2 + creator + 미배포 + first-writer-wins를 전부 수행한다. finalize에서 CREATE2를 다시 계산해봐야 얻는 정보는 "요청자가 salt를 안다" 뿐인데, 이는 배포 후엔 공격자도 만족시키는 **바로 그 폐기된 신호**다. finalize의 진짜 권한 근거는 "이 token_id가 이 세션 소유로 (배포 전에) 예약되었는가" 이며 이는 예약 조회로 완결된다. CREATE2를 finalize에 남기면 *false sense of defense-in-depth* — 이미 무력화된 검사를 중복으로 도는 셈이라 오히려 리뷰어를 오도한다.
- **단순성 + I1 소멸**: finalize에서 salt/creator/name/symbol/metadata_uri/**version** 을 전부 제거하면 finalize의 공격 표면과 파라미터 파싱이 사라지고, version 기본값(I1) 문제도 통째로 사라진다.
- **코드 재사용(비용 최소)**: CREATE2 재계산 로직(`SaltService::compute_token_address`)과 순수 검증 헬퍼(`creator_matches`/`address_matches`/`canonical_persist_ids`)는 **삭제하지 않고 reserve로 호출자만 이동**한다. 기존 리뷰 통과한 코드/테스트가 그대로 유효하다(§Task 12 참조).

**결론**: finalize = **reservation-only**. CREATE2·creator 방어는 시스템에서 사라지는 게 아니라 "배포 전"이라는 올바른 시점(reserve)으로 이동한다. 진짜 보안 경계는 `eth_getCode 미배포 확인 + token_id 배포전 비밀성 + first-writer-wins` 이다.

### 7.2 HIGH — OAuth 콜백 리다이렉트는 open redirect 금지 (설계 확정, 아래 반영)

콜백(`GET /x/oauth/callback`) 처리 후 프론트로 리다이렉트하는 URL은 **클라이언트가 지정할 수 없다.** `/x/oauth/login` 요청에 `return_to` 같은 파라미터를 받지 않으며, 성공/실패 리다이렉트 대상은 서버 env(`X_OAUTH_REDIRECT_SUCCESS_URL`, `X_OAUTH_REDIRECT_FAILURE_URL`)에 고정된 단일 URL만 사용한다. (open redirect는 피싱에 악용 가능한 OWASP 항목.)

### 7.3 MEDIUM — 후속 조치 필요

- `/x/followed-by`의 `handle`은 X API 호출 전에 `^[A-Za-z0-9_]{1,15}$`(X 핸들 형식)로 검증한다. 형식 불일치는 X API 호출 없이 즉시 `400`.
- `token_x_followed_by`는 확정 시점의 스냅샷이라 이후 대상 계정이 언팔로우/삭제/개명해도 갱신되지 않는다. "현재 팔로우 중"이 아니라 "`checked_at` 시점에 확인됨"이라는 것을 API 응답과 UI 문구에 명시한다(재검증 배치는 이번 범위 밖).

### 7.4 LOW — 기록만 해둠 (별도 조치 불필요)

- X access/refresh_token은 Redis에 평문 저장되지만 30분 TTL이고 Postgres엔 저장하지 않음 — 기존 nonce 패턴과 동일한 신뢰 경계, 새로운 구멍 아님.
- 레이트리밋이 계정 단위라 다중 지갑+다중 X 계정으로 우회 가능하나, X 계정 확보 비용이 억제력으로 작용해 MVP 범위에서는 허용.

### 7.5 (개정 2026-07-03) reserve 도입에 따른 새 위협모델

- **CRITICAL — 온체인 미배포 확인은 fail-closed.** reserve의 `eth_getCode(token_id)` 호출이 실패(체인 노드 장애/타임아웃/네트워크)하면 **"미배포"로 낙관 가정하면 안 된다.** RPC 오류는 "배포 여부 확인 불가"로 간주해 예약을 **거부**(HTTP 503 `onchain_check_unavailable`)한다. 낙관 가정 시, 노드 장애를 유발/이용한 공격자가 이미 배포된(salt 공개된) 토큰을 예약해버릴 수 있다 — 정확히 C1을 되살리는 구멍이다. 프론트는 503을 받으면 잠시 후 재시도한다.
- **HIGH — reserve rate limit.** reserve는 온체인 RPC 1회 + DB write를 유발하므로, 공격자가 무의미한 token_id를 대량으로 던져 RPC/DB를 소모(amplification)하거나 예약 테이블을 오염시키는 것을 막아야 한다. `account_id`당 **분당 5회**로 제한(`X_RESERVE_RATE_LIMIT`, 기존 `check_and_increment` 재사용). 세션 필수라 rate-limit 키는 세션 지갑주소 기준.
- **MEDIUM — first-writer-wins의 원자성.** 예약 INSERT는 반드시 **단일 원자적 문**(`ON CONFLICT (token_id)` 활용)으로 수행해 "조회 후 삽입" 사이의 race를 없앤다. 두 계정이 동시에 같은 token_id를 예약하려 해도 정확히 한 계정만 소유자가 되고, 나머지는 409를 받는다. row는 생성 후 `account_id` 가 절대 바뀌지 않으므로(불변) 이후 조회는 항상 진짜 first-writer를 돌려준다.
- **LOW — 고아 예약 row.** 예약만 하고 finalize/배포를 끝내 안 하면 예약 row가 고아로 남는다. 이는 §2의 고아 verification과 동일한 성격(salt 마이닝이 이미 갖는 리스크)이라 새 문제 아님. TTL/정리 배치는 범위 밖. 고아 예약은 해당 token_id를 영구히 그 계정에 묶어두지만, token_id는 배포전 비밀이므로 타인에게 해가 없다.
- **정보노출 없음**: reserve/finalize 응답은 `{ok:true}` 뿐이고 예약 소유자(account_id)나 x_user_id를 노출하지 않는다. 409 응답도 소유자 주소를 본문에 담지 않는다(`token_already_reserved` 코드만).

---

## 8. 에러 처리 / 레이트리밋 / 기타 보안

**에러:**
- OAuth state 만료/불일치 → 프론트로 `?x_verify_error=state_expired` 리다이렉트, 처음부터 재시도
- `/x/followed-by` 4번째 시도 → `400 { error: "max_followed_by_reached" }`
- `check_follows_me` 결과 `is_following=false` → `200`으로 결과만 반환(`{is_following: false}`), 저장하지 않음
- `finalize` 시 pending 만료/없음 → `410 { error: "verification_expired" }`, 프론트가 "Verify with X" 재시작 안내
- `finalize` 시 CREATE2 재계산 불일치 또는 `creator` != 세션 지갑주소 → `403 { error: "creator_mismatch" }` (§7.1)
- X API 장애/rate limit → 재시도 없이 즉시 에러 반환(가격 신호와 달리 캐시로 완화할 성격이 아니므로 사용자가 재시도)

**(개정 2026-07-03) reserve/finalize 에러:**
- `reserve` `creator` != 세션 또는 `CREATE2(version,salt)` != `token_id` → `403 { error: "creator_mismatch" }`
- `reserve` `eth_getCode(token_id)` 비어있지 않음(이미 배포) → `409 { error: "already_deployed" }`
- `reserve` 다른 계정이 이미 선점 → `409 { error: "token_already_reserved" }`
- `reserve` 온체인 확인 RPC 실패(fail-closed) → `503 { error: "onchain_check_unavailable" }`
- `finalize` (개정) 예약 없음 또는 `reservation.account_id` != 세션 → `403 { error: "not_reserved" }` (기존 `creator_mismatch` 대체)
- `finalize` (개정) pending 만료/없음 → `410 { error: "verification_expired" }` (동일)
- 위 §8 최초 목록의 "`finalize` 시 CREATE2 재계산 불일치 …" 항목은 폐기(그 검증이 reserve로 이동).

**레이트리밋** (기존 `redis.incr_with_expire(key, ttl_secs)` 재사용):
- `/x/oauth/login`: `account_id`당 분당 3회
- `/x/followed-by`: `account_id`당 분당 5회
- `/x/verification/reserve`: `account_id`당 분당 5회 (개정, `X_RESERVE_RATE_LIMIT`)

**보안:**
- PKCE `state`가 CSRF 토큰 역할 겸함. `/x/oauth/callback`은 X가 보내는 외부 요청이라 기존 Origin 헤더 검증이 적용되지 않으므로, state 검증이 유일한 방어선 — 반드시 1회성(`GETDEL`)으로 소비한다.
- `X_CLIENT_SECRET`은 서버 env에만 존재, 프론트/응답 어디에도 노출하지 않는다.
- 창작자의 실제 `x_handle`/`x_user_id`는 어떤 응답에도 포함하지 않는다. `followed_by`에 노출되는 핸들은 창작자 본인이 아니라 창작자를 팔로우하는 제3자 핸들이므로 공개되는 게 의도된 동작이다.

---

## 9. 오픈 이슈 / 후속 확인 필요

- 코인 생성을 여러 번 시도(재도전)할 때, 이미 finalize된 `account_id`가 새 코인을 또 만들면 X 재인증을 처음부터 다시 해야 한다(MVP 범위, 토큰 저장 안 하므로). 필요하면 추후 refresh_token 기반 "재확인만" 플로우 추가 검토.
- `x-api`(`/Users/gyu/project/nads-pump/x-api`)는 프로덕션용이 아닌 테스트 하네스이므로 구현 시 그대로 이식하지 말고 위 설계에 맞게 다시 작성한다(세션은 지갑 세션에 종속, 인메모리 대신 Redis).
