# Dev Post API 문서

## 개요

Dev Post는 코인의 온체인 **creator**가 자신의 코인에 글을 올리는 기능입니다. 글(post)은 텍스트 본문 +
선택적 이미지(최대 4장) + 선택적 임베드 트윗(본문에서 파싱) + 선택적 **투표(poll)**(2~3개 옵션, 생성 14일
후 자동 마감)로 구성됩니다. 지갑이 연결된 아무 사용자나 게시물을 **좋아요(토글)** 하거나 **투표**(마감
전까지 변경 가능)할 수 있습니다.

세 가지 집계 뷰를 제공합니다.
- **전체 피드**: 모든 코인의 Dev Post를 최신순으로 (또는 `token_id`로 특정 코인만 필터)
- **Trending**: 최근 7일 좋아요 수 기준 상위 3개 게시물 (부족하면 최신 게시물로 채움)
- **Ranking**: 코인별 누적 좋아요 수 기준 "Top Active Dev" 랭킹

전체 설계 배경은 `docs/plans/2026-07-07-dev-post-api-design.md` 참고. 이 문서는 **구현된 API의
레퍼런스**입니다.

### 용어
- **Dev / creator**: 코인의 현재 `token.creator`(EIP-55). 해당 코인의 Dev Post 생성/수정/삭제는
  creator만 가능. `token.creator`는 변경될 수 있으며(`set_creator_history`), 인가는 항상 **현재** 값 기준.
- **author**: 게시물 작성 당시의 creator 주소. 이후 코인 creator가 바뀌어도 과거 게시물의 author는
  그대로 유지됨(작성 당시 실제 작성자 보존).
- **Post**: `dev_post` 한 row (본문 + 이미지 0장 이상 + 선택적 poll). 정확히 하나의 `token_id`에 속함.
- **Poll**: 게시물당 최대 1개. 옵션 2~3개, 생성 14일 후 마감.

---

## 인증 개요

| 구분 | 엔드포인트 | 인증 |
|---|---|---|
| 공개 (optional-auth) | `GET /dev-post`, `GET /dev-post/trending`, `GET /dev-post/ranking`(개인화 없음), `GET /dev-post/{post_id}` | 불필요. 세션 쿠키가 있으면 `liked_by_me`/`my_vote_option`을 채움 |
| 보호 (세션 필수) | `POST /dev-post/image`, `POST /dev-post`, `PATCH`/`DELETE /dev-post/{post_id}`, `PUT`/`DELETE /dev-post/{post_id}/pin`, `POST`/`DELETE /dev-post/{post_id}/like`, `POST /dev-post/{post_id}/vote` | 필수 (지갑 로그인 세션 쿠키, `authenticate_user` 미들웨어) |

공개 GET들은 세션 쿠키가 있으면 `optional_session_address` 헬퍼로 주소를 읽어 개인화 필드를 채우고,
없거나 무효해도 **401을 반환하지 않고** `liked_by_me: false` / `my_vote_option: null`로 응답합니다.
`GET /dev-post/ranking`은 코인 단위 집계라 개인화 필드 자체가 없습니다.

모든 요청의 EVM 주소(`token_id` 등)는 핸들러 진입 시 `valid_account_id`/`valid_existing_token_id`로
EIP-55 체크섬 정규화됩니다.

---

## API 엔드포인트

### 1. 피드 조회 (`GET /dev-post`)

전체 Dev Post 피드. `token_id`를 지정하면 해당 코인의 게시물만 (코인 상세 페이지 "Learn More" 용도).
`token_id` 없이 호출하면 전체 "All Dev Posts" 피드. 최신순(`id DESC`) 정렬.

#### 요청
- **Method**: `GET`
- **인증**: 불필요 (optional-auth)

#### Query Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|---|---|---|---|
| `token_id` | string | X | 특정 코인으로 필터 (EIP-55 정규화, 존재하는 토큰이어야 함) |
| `page` | integer | X | 페이지 번호, 기본 1, 최소 1 |
| `limit` | integer | X | 페이지당 개수, 기본 10, 1~100 |

> `direction` 파라미터도 파싱은 되지만(하우스 공통 `PaginationParams`를 재사용하기 때문) 피드 정렬에는
> 반영되지 않습니다 — 항상 최신순(`id DESC`) 고정입니다.

#### 응답
```json
{
  "pin": null,
  "posts": [
    {
      "id": "123456789",
      "token": { "token_id": "0x…", "name": "…", "symbol": "…", "image_uri": "https://…", "market_cap": "1000000000000000000000000" },
      "author": { "account_id": "0x…", "nickname": "creator.eth", "image_uri": "https://…" },
      "title": "Announcement title",
      "body": "gm holders, check this out",
      "tweet_url": null,
      "images": ["https://storage.nadapp.net/devpost/…"],
      "poll": null,
      "like_count": 12,
      "liked_by_me": false,
      "is_edited": false,
      "created_at": "2026-07-07T00:00:00Z",
      "updated_at": "2026-07-07T00:00:00Z"
    }
  ],
  "total_count": 240
}
```

`token_id`가 있는 토큰 피드는 active pin을 일반 게시물과 별도로 반환합니다. 1페이지의 `pin`은
`posts`의 `limit`개에 **추가**되는 항목이며, 2페이지부터는 `pin: null`입니다. 모든 토큰 페이지는 active
pin을 `total_count` 계산과 `offset`/`limit` 적용보다 먼저 일반 게시물 집합에서 제외하므로, pin이 페이지
사이에서 중복되거나 일반 페이지네이션을 밀어내지 않습니다. pin이 없는 토큰은 모든 페이지에서
`pin: null`입니다.

`token_id`가 없는 전체 피드는 항상 `pin: null`이며, pinned post도 일반 게시물과 똑같이 `posts`와
`total_count`에 포함합니다.

페이지 1이면서 기본 `limit=10`인 피드만 Redis cache-aside를 사용합니다. 캐시 payload는
`liked_by_me`/`my_vote_option`을 넣지 않는 viewer-neutral base이고, 응답 직전에 요청 viewer의 개인화를
적용하므로 사용자 간 상태가 섞이지 않습니다. 읽기/쓰기는 `devpost:feed:v2:*` v2 키를 사용합니다. 캐시는
설정된 TTL 동안 eventually consistent이며 운영 설정은 최대 60초(`DEVPOST_FEED_EXPIRATION <= 60000`)로
제한합니다. pin PUT/DELETE는 해당 token feed 키만 무효화하고 global feed, 다른 token, detail,
trending/ranking 키는 건드리지 않습니다.

#### 에러 응답
- `400`: `token_id` 형식이 잘못됨, 또는 `page`/`limit` 범위 오류
- `404`: `token_id`로 지정한 토큰이 존재하지 않음
- `500`: 내부 서버 에러

---

### 2. Trending 조회 (`GET /dev-post/trending`)

"Trending Dev Posts": 최근 7일 좋아요 수 기준 상위 3개 게시물 (고정 크기, 페이지네이션 없음). 좋아요가
부족해 3개를 못 채우면(초기 상태 등) 나머지는 최신 게시물로 채워 섹션이 비지 않도록 합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요 (optional-auth)

#### 응답
```json
{
  "posts": [ /* DevPostResponse, 최대 3개, 좋아요 수 내림차순 우선 + 부족분은 최신순 */ ]
}
```

#### 에러 응답
- `500`: 내부 서버 에러

---

### 3. Ranking 조회 (`GET /dev-post/ranking`)

"Top Active Dev Ranking": 코인별 (삭제되지 않은 게시물의) 누적 좋아요 수 기준 랭킹.

#### 요청
- **Method**: `GET`
- **인증**: 불필요 (개인화 필드 없음)

#### Query Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|---|---|---|---|
| `page` | integer | X | 페이지 번호, 기본 1, 최소 1 |
| `limit` | integer | X | 페이지당 개수, 기본 10, 1~100 |

> 피드와 마찬가지로 `direction`은 정렬에 영향 없음 — 항상 `total_likes DESC, last_posted_at DESC,
> token_id ASC` 고정. 마지막 `token_id`는 유니크 타이브레이크로, 동점 코인이 페이지 경계에서 중복되거나
> 누락되지 않도록 보장합니다.

#### 응답
```json
{
  "rankings": [
    {
      "rank": 1,
      "token": { "token_id": "0x…", "name": "…", "symbol": "…", "image_uri": "https://…", "market_cap": "15875.719277088" },
      "total_likes": 900,
      "post_count": 12,
      "last_posted_at": "2026-07-07T00:00:00Z"
    }
  ],
  "total_count": 37
}
```

#### 에러 응답
- `400`: `page`/`limit` 범위 오류
- `500`: 내부 서버 에러

---

### 4. 게시물 상세 조회 (`GET /dev-post/{post_id}`)

#### 요청
- **Method**: `GET`
- **인증**: 불필요 (optional-auth)

#### Path Parameters
| 파라미터 | 타입 | 설명 |
|---|---|---|
| `post_id` | string (BIGINT) | 게시물 ID |

#### 응답
`DevPostResponse` 단일 객체 (모양은 [응답 타입](#typescript-interfaces) 참고).

#### 에러 응답
- `404`: 게시물이 없거나 삭제됨
- `500`: 내부 서버 에러

---

### 5. 이미지 업로드 (`POST /dev-post/image`)

본문/poll 옵션에 붙일 이미지를 R2에 업로드하고 URL을 돌려받습니다. 멀티파트가 아니라 **raw 바이너리
body** 방식입니다. 생성/수정 API를 호출하기 전에 먼저 이 엔드포인트로 이미지를 올리고, 반환된
`image_uri`를 `image_uris`/`poll.options[].image_uri`에 담아 보내는 2단계 흐름입니다. 생성/수정 API는
**이 엔드포인트가 돌려준 URL만 받습니다** (아래 도메인 allowlist 참고).

#### 요청
- **Method**: `POST`
- **Content-Type**: **무시됩니다.** 포맷은 파일의 매직 바이트로 판별하며(`/metadata/image`와 동일),
  판별된 타입으로 R2에 저장됩니다. 허용: `image/jpeg` | `image/png` | `image/webp` | `image/svg+xml`
- **인증**: 필수 (세션이 있는 아무 지갑 — creator 여부 검사 없음)
- **Body**: 이미지 원본 바이너리, 최대 5MB (프레임워크 레벨 `DefaultBodyLimit` 5,000,000바이트로
  1차 컷, 핸들러 내부에서 5×1024×1024바이트로 2차 체크 — 실질적으로 프레임워크 리밋이 먼저 걸림)

> 헤더를 믿지 않는 이유: `Content-Type`은 클라이언트가 정하는 값이고, 그대로 R2 객체의 `Content-Type`이
> 되어 나중에 브라우저가 그 타입으로 해석합니다. 헤더를 신뢰하면 임의 바이트를 임의 타입으로 호스팅하는
> 엔드포인트가 됩니다.

**NSFW 검사**: 포맷 판별 직후 AWS Rekognition으로 성인물 여부를 검사하고, 판정되면 **R2에 올리지 않고
400으로 거부**합니다. SVG도 PNG로 렌더링한 뒤 검사하므로, SVG로 감싸 넣은 사진도 그대로 잡힙니다.
`/metadata/image`(토큰 이미지)는 거부하지 않고 `is_nsfw` 플래그만 달아 통과시키는데(프론트 블러 전제),
dev-post는 게시물에 음란물을 아예 싣지 않는 정책이라 즉시 거부입니다. 그래서 dev-post 응답에는
`is_nsfw` 필드가 없습니다.

**SVG 저장 방식**: SVG는 `Content-Disposition: attachment`로 저장됩니다. SVG는 `<script>`를 품을 수
있어서, 객체 URL을 브라우저에서 직접 열면 우리 도메인(`storage.nadapp.net`)에서 스크립트가 실행되기
때문입니다. 이 헤더는 `<img src>` 렌더링에는 영향을 주지 않으므로 게시물 표시는 그대로입니다.

#### 응답
```json
{ "image_uri": "https://storage.nadapp.net/devpost/{uuid}" }
```

#### 에러 응답
- `400`: 바이트가 지원 이미지 포맷이 아님 (`Invalid image format` / `Unsupported image type: …` /
  `File too small` / `Invalid SVG format`), 또는 NSFW 판정
  (`Image rejected: detected as inappropriate (NSFW) content`)
- `413`: 5MB 초과 (프레임워크 body limit)
- `500`: R2 업로드 실패 (Rekognition 호출 실패 포함)

---

### 6. 게시물 생성 (`POST /dev-post`)

**creator 전용.** `token_id`의 현재 `token.creator`가 세션 주소와 같아야 하며, 이 확인은 **write
pool**에서 이루어집니다(replica lag로 인한 오탐 방지).

#### 요청
- **Method**: `POST`
- **Content-Type**: `application/json`
- **인증**: 필수, `session_address == token.creator`

#### Request Body
```json
{
  "token_id": "0x…",
  "title": "Announcement title",
  "body": "gm holders …",
  "image_uris": ["https://storage.nadapp.net/devpost/…"],
  "poll": {
    "options": [
      { "label": "Option A", "image_uri": "https://…" },
      { "label": "Option B" }
    ]
  }
}
```

| 필드 | 타입 | 필수 | 설명 |
|---|---|---|---|
| `token_id` | string | O | 대상 코인 (EIP-55, 존재+creator==caller 확인) |
| `title` | string | O | 제목. 누락/null/빈 문자열 또는 공백만이면 400. 입력 공백·줄바꿈은 그대로 보존 |
| `body` | string | X | 설명(description) 본문. `x.com`/`twitter.com` 링크가 있으면 응답에서 `tweet_url`로 파싱됨 |
| `image_uris` | string[] | X | `POST /dev-post/image`에서 받은 URL, 0~4개 |
| `poll` | object | X | 옵션 2~3개. 각 옵션 `label`(필수, 공백만은 불가) + `image_uri`(선택) |

**최소 1개 규칙**: required `title`과 함께 `body`(공백 제외 비어있지 않음) / `image_uris`(1개 이상) /
`poll` 중 **최소 하나**는
있어야 합니다. 셋 다 없으면 400.

**이미지 URI allowlist**: `image_uris[]`와 `poll.options[].image_uri`는 반드시
`https://storage.nadapp.net/devpost/{uuid}` 형태여야 합니다 — 접두사뿐 아니라 뒤의 오브젝트 키까지
`POST /dev-post/image`가 실제로 발급하는 형태(하이픈 UUID)와 정확히 일치해야 합니다. 외부 URL,
`javascript:`/`data:` 스킴, 다른 경로(`/account/…` 등), 키에 임의 문자가 섞인 URL은 모두 400입니다.
즉 이미지는 `POST /dev-post/image`를 거쳐야만 게시물에 붙일 수 있습니다.

#### 응답
`DevPostResponse` (생성 직후 상태 그대로 조회해 반환).

#### 에러 응답
- `400`: 유효성 검증 실패 (title 누락/null/blank, 완전히 빈 post, 이미지 5개 이상, poll 옵션 개수/라벨 오류, 잘못된 `token_id` 형식)
- `413`: JSON 요청 body가 100,000 bytes를 초과
- `401`: 세션 없음
- `403`: 호출자가 해당 코인의 creator가 아님
- `404`: `token_id`가 존재하지 않음
- `500`: 내부 서버 에러

---

### 7. 게시물 수정 (`PATCH /dev-post/{post_id}`)

**author 전용.** 제목/description 본문/이미지를 수정할 수 있으며 **poll은 생성 후 불변**입니다.

#### 요청
- **Method**: `PATCH`
- **인증**: 필수, `dev_post.author == session_address`

#### Request Body
```json
{ "title": "업데이트된 제목", "body": "업데이트된 설명", "image_uris": ["https://…"] }
```
- `title`/`body`/`image_uris` 각각 생략 가능 — 생략한 필드는 변경하지 않습니다. `title`은 tri-state입니다:
  생략(유지), `null`(400), 문자열 값(공백만이면 400, 그 외 그대로 교체).
- `body`는 title과 독립적인 description-only 필드입니다. 모든 필드를 생략하면 400입니다.
- `image_uris`를 보내면 **전체 교체**(기존 이미지 목록을 지우고 새로 삽입)입니다. 부분 추가/삭제가 아닙니다.
- 성공 시 `edited_at = now()` 설정 → 응답의 `is_edited: true`.

#### 응답
`DevPostResponse` (수정 후 최신 상태).

#### 에러 응답
- `400`: 유효성 검증 실패 (title null/blank, 모든 필드 생략, 이미지 5개 이상)
- `413`: JSON 요청 body가 100,000 bytes를 초과
- `401`: 세션 없음
- `403`: 세션 주소가 게시물 author가 아님
- `404`: 게시물이 없거나 이미 삭제됨
- `500`: 내부 서버 에러

---

### 8. 게시물 삭제 (`DELETE /dev-post/{post_id}`)

**author 전용.** 소프트 삭제(`deleted_at = now()`) — 이후 피드/상세/trending/ranking/좋아요·투표 집계
어디에도 나타나지 않습니다. 이미 삭제된 게시물을 다시 삭제 요청하면 404.

#### 요청
- **Method**: `DELETE`
- **인증**: 필수, `dev_post.author == session_address`

#### 응답
```json
{ "deleted": true }
```

#### 에러 응답
- `401`: 세션 없음
- `403`: 세션 주소가 게시물 author가 아님
- `404`: 게시물이 없거나 이미 삭제됨
- `500`: 내부 서버 에러

---

### 9. 게시물 pin 설정 (`PUT /dev-post/{post_id}/pin`)

현재 coin creator가 자신의 새 공지를 token feed 상단에 고정합니다. 호출자는 **현재**
`token.creator`여야 하고, 대상은 같은 token에 속하면서 삭제되지 않았고 현재 creator가 직접 작성한 live
post여야 합니다. 최초 pin, 동일 post 반복 PUT, 다른 적격 post로의 교체는 모두 빈 body의
`204 No Content`를 반환합니다. token별 active pin은 정확히 하나입니다.

creator가 이전되면 기존 pin mapping은 유지됩니다. 이전 creator는 더 이상 pin을 변경할 수 없고, 새
creator는 기존 pin을 제거할 수 있습니다. 교체는 새 creator가 자신이 작성한 새 post로만 할 수 있으며,
새 creator가 이전 creator의 과거 post를 다시 pin할 수는 없습니다.

#### 요청
- **Method**: `PUT`
- **인증**: 필수, `session_address == token.creator == dev_post.author`

#### 응답
빈 body의 `204 No Content`.

#### 에러 응답
- `400`: `post_id` 형식이 잘못됨
- `401`: 세션 없음
- `403`: 호출자가 현재 creator가 아니거나 post author가 현재 creator가 아님
- `404`: live post 또는 token이 존재하지 않음
- `500`: 내부 서버 에러

---

### 10. 게시물 pin 해제 (`DELETE /dev-post/{post_id}/pin`)

현재 coin creator만 호출할 수 있습니다. URL의 exact post가 현재 pin이면 제거하고, 이미 pin이 없거나
다른 post가 현재 pin인 경우에는 no-op입니다. 대상 post가 live인 동안 exact-post 호출과 반복 호출은 모두
빈 body의 `204 No Content`를 반환합니다. 이 동작은 오래된 클라이언트의 `DELETE A`가 이미 교체된 현재
pin `B`를 지우지 않게 합니다.

#### 요청
- **Method**: `DELETE`
- **인증**: 필수, `session_address == token.creator` (post author와 같을 필요 없음)

#### 응답
빈 body의 `204 No Content`.

#### 에러 응답
- `400`: `post_id` 형식이 잘못됨
- `401`: 세션 없음
- `403`: 호출자가 현재 creator가 아님
- `404`: live post 또는 token이 존재하지 않음
- `500`: 내부 서버 에러

게시물을 소프트 삭제할 때는 pin mapping 제거와 `deleted_at` 갱신을 **하나의 SQL statement(CTE)** 로 처리합니다.
단일 statement는 그 자체가 암묵적 트랜잭션이므로 둘은 atomic하게 적용되며, 삭제된 post가 pinned 상태로 남지
않습니다. 운영 pgbouncer가 statement pooling mode라 명시적 `BEGIN`/`COMMIT`을 쓸 수 없어, dev-post의 모든
write 경로가 이 방식을 따릅니다.

### 11. 좋아요 (`POST /dev-post/{post_id}/like`)

토글의 "on" 쪽. 아무 지갑이나 가능. `(post_id, account_id)` 유니크 — 이미 좋아요한 상태에서 다시
호출해도 멱등(`INSERT ... ON CONFLICT DO NOTHING`).

#### 요청
- **Method**: `POST`
- **인증**: 필수 (creator 여부 무관, 아무 지갑)

#### 응답
```json
{ "like_count": 129, "liked_by_me": true }
```

#### 에러 응답
- `401`: 세션 없음
- `404`: 게시물이 없거나 삭제됨 — 존재/미삭제 여부를 먼저 확인한 뒤 좋아요를 기록함
- `500`: 내부 서버 에러

---

### 12. 좋아요 취소 (`DELETE /dev-post/{post_id}/like`)

토글의 "off" 쪽. `DELETE FROM dev_post_like WHERE post_id=… AND account_id=…` — 멱등.

#### 요청
- **Method**: `DELETE`
- **인증**: 필수

#### 응답
```json
{ "like_count": 128, "liked_by_me": false }
```

> **좋아요와의 비대칭**: 좋아요(`POST`)는 대상 게시물이 존재/미삭제인지 먼저 확인해 404를 낼 수
> 있지만, 좋아요 취소(`DELETE`)는 게시물 존재 여부를 확인하지 않습니다 — 존재하지 않거나 이미 삭제된
> `post_id`를 넘겨도 그냥 삭제(no-op)가 성공하고 그 시점의 `like_count`를 돌려줍니다. 404가 나지 않습니다.

#### 에러 응답
- `401`: 세션 없음
- `500`: 내부 서버 에러

---

### 13. 투표 (`POST /dev-post/{post_id}/vote`)

`(post_id, account_id)`당 한 표, **마감 전까지 변경 가능**(upsert). 아무 지갑이나 가능.

#### 요청
- **Method**: `POST`
- **인증**: 필수

#### Request Body
```json
{ "option_position": 2 }
```

#### 처리
1. 해당 게시물의 poll `closes_at` 조회 (게시물이 삭제됐거나 poll이 없으면 실패)
2. `closes_at <= now()` 면 마감된 것으로 거부
3. `option_position`이 그 poll의 실제 옵션인지 확인
4. `(post_id, account_id)` PK로 upsert — 이미 투표했으면 옵션 변경, 처음이면 신규 삽입

#### 응답
```json
{
  "total_votes": 43,
  "options": [
    { "position": 1, "label": "Option A", "image_uri": "https://…", "vote_count": 30 },
    { "position": 2, "label": "Option B", "image_uri": null, "vote_count": 13 }
  ],
  "my_vote_option": 2
}
```

#### 에러 응답
- `400`: `option_position`이 그 poll에 존재하지 않는 옵션
- `401`: 세션 없음
- `404`: 게시물에 poll이 없음, 게시물이 삭제됨, 또는 게시물 자체가 없음 (셋 다 "Poll not found"로 동일하게 처리)
- `409`: poll이 이미 마감됨 (`closes_at <= now()`)
- `500`: 내부 서버 에러

---

### CMS moderation (관리자)

CMS 관리자는 CMS 인증과 EIP-55 canonical admin 주소로 삭제/복구를 수행합니다. 일반 author
`DELETE /dev-post/{post_id}`와 혼동하지 마세요. 일반 삭제는 작성 당시 author만 수행할 수 있습니다.

### 14. CMS 게시물 삭제 (`DELETE /cms/dev-post/{post_id}`)

CMS 관리자가 게시물을 삭제합니다. content(`title`/`body`/images/poll)와 관계 행은 보존하고, 해당
게시물의 pin mapping만 조건부로 제거합니다. 물리 `dev_post` row가 존재하면 이미 삭제된 상태도
감사되는 멱등 no-op(`changed=false`)으로 처리하고 빈 `204`를 반환합니다.

#### 요청

- **Method**: `DELETE`
- **인증**: 필수, 관리자 세션 주소가 EIP-55 canonical admin 주소와 일치해야 함
- **Request Body**: 없음

#### Path Parameters

| 이름 | 타입 | 설명 |
|---|---|---|
| `post_id` | BIGINT string | 삭제할 Dev Post ID |

#### 응답

빈 body의 `204 No Content`.

#### 에러 응답

- `400`: `post_id` 형식이 잘못됨
- `401`: 관리자 세션 없음
- `403`: 호출자가 관리자가 아님
- `404`: 물리 `dev_post` row가 없음
- `500`: 내부 서버 에러

admin-first deterministic lock order와 moderation audit log를 사용합니다. audit 쓰기는 idempotent이며,
commit outcome-unknown은 reconciliation/retry 대상입니다.

---

### 15. CMS 게시물 복구 (`POST /cms/dev-post/{post_id}/restore`)

CMS 관리자가 삭제된 게시물을 복구합니다. content(`title`/`body`/images/poll)와 관계 행은 삭제 전
상태로 보존됩니다. 물리 `dev_post` row와 token row가 존재하면 이미 live/복구된 상태도 감사되는
멱등 no-op(`changed=false`)으로 처리하고 빈 `204`를 반환합니다.

#### 요청

- **Method**: `POST`
- **인증**: 필수, 관리자 세션 주소가 EIP-55 canonical admin 주소와 일치해야 함
- **Request Body**: 없음

#### Path Parameters

| 이름 | 타입 | 설명 |
|---|---|---|
| `post_id` | BIGINT string | 복구할 Dev Post ID |

#### 응답

빈 body의 `204 No Content`.

#### 에러 응답

- `400`: `post_id` 형식이 잘못됨
- `401`: 관리자 세션 없음
- `403`: 호출자가 관리자가 아님
- `404`: 물리 `dev_post` row가 없음
- `409`: 복구에 필요한 token row를 사용할 수 없음
- `500`: 내부 서버 에러

admin-first deterministic lock order와 moderation audit log를 사용합니다. audit 쓰기는 idempotent이며,
commit outcome-unknown은 reconciliation/retry 대상입니다.

### CMS 운영 전환 및 관찰

- 모든 Dev Post 읽기/쓰기와 CMS 제어, 직접 API caller를 함께 gate하고 pre-title 노드를 drain합니다.
  audit-schema readiness와 소비자 공지를 완료한 뒤 운영 전환을 시작합니다.
- row count, disposable/representative samples를 확인하되 production inserts는 만들지 않습니다.
  backup/WAL/free-space와 replica health도 함께 확인합니다.
- global feed, token feed, detail, trending의 네 cache TTL은 모두 `<=60_000ms`여야 합니다.
  migration `0041` 적용은 **point of no return**입니다. 적용 후 title schema와 row count,
  title/disposable fixture shape, audit schema, replica convergence를 확인하고 title-aware binary만
  배포합니다.
- 알려진 cache key는 `REDIS_KEY_PREFIX`를 적용한 다음 exact-key 삭제만 수행합니다. 삭제 집합은
  global feed의 `devpost:feed:global`, `devpost:feed:v2:global`, `devpost:feed:v3:global`, 해당 token
  feed의 `devpost:feed:{token_id}`, `devpost:feed:v2:{token_id}`, `devpost:feed:v3:{token_id}`,
  detail의 `devpost:detail:{post_id}`, `devpost:detail:v2:{post_id}`, trending의
  `devpost:trending`, `devpost:trending:v2`입니다. legacy/v2 key는 fallback source가 아닙니다.
  위 10개 exact key 삭제를 모두 완료한 뒤에만 `devpost:ranking:generation`을 증가시키며 payload는
  `devpost:ranking:v2:{generation}:{page}:{limit}`을 사용합니다. 알려지지 않은 key만 자연
  만료시키고 wildcard 열거/삭제는 하지 않습니다.
- 인증된 create/edit/read/title-validation/auth-precedence smoke test를 통과한 뒤 CMS controls를
  활성화합니다.
- migration commit 전에는 traffic을 gate한 채 transaction을 abort/rollback하고 pre-title binary로
  복구할 수 있습니다. commit 후에는 pre-title binary 실행, body 재결합, title 제거,
  compatibility responses 복구를 금지하고 title-aware corrective binary로 roll-forward합니다.
- CMS controls만 비활성화하는 것은 audit/content가 authoritative로 유지되는 동안 안전합니다.
  pgactive 제약과 conditional pin restore 경계를 운영 점검에 포함합니다.
- outcome-unknown, audit/reconciliation, lock/deadlock, replica lag, cache PTTL/late-fill,
  old-schema cache access, title/body rendering을 모니터링합니다.

---

## 응답 필드 설명 (`DevPostResponse`)

| 필드 | 설명 |
|---|---|
| `id` | BIGINT를 **문자열로 직렬화** (snowflake id가 JS `Number` 2^53 정밀도를 초과하므로) |
| `token` | 코인 요약 (`token_id`/`name`/`symbol`/`image_uri`/`market_cap`) — `market_cap`은 market×price 조인 기반 **USD 마켓캡** (= hype `market_cap_usd` 의미). **스케일 주의: 랭킹(`GET /dev-post/ranking`)은 사람이 읽는 USD 문자열(예: `"15875.719277088"`), 피드/상세/트렌딩은 아직 raw ×10^18 스케일** — 후속에서 사람 단위로 통일 예정. market 행 없으면 `null`, quote USD 가격 없으면 `"0"`. 캐시 TTL(≤60s)만큼 지연 가능 |
| `author` | 게시물 작성자 요약 (작성 당시 creator, 이후 creator가 바뀌어도 유지) |
| `title` | 제목. 입력 공백·줄바꿈을 보존하며 body와 별도 필드 |
| `body` | description-only 본문 원문 (트윗 링크 포함 가능) |
| `tweet_url` | 본문에서 파싱한 첫 `https://x.com/…` 또는 `https://twitter.com/…` URL, 없으면 `null`. 네트워크 조회 없음 — 프론트가 fxtwitter 등으로 렌더링 |
| `images` | 이미지 URL 배열, 0~4개, 저장 순서(`position`) 그대로 |
| `poll` | 없으면 `null`. `closes_at`(생성 + 14일), `is_closed`(읽기 시점에 `closes_at <= now()`로 계산 — 별도 cron/배치 없음), `total_votes`, `options[]`, `my_vote_option`(비로그인/미투표 시 `null`) |
| `like_count` | `dev_post_like` derived COUNT(*) |
| `liked_by_me` | 비로그인 시 `false` |
| `is_edited` | `edited_at IS NOT NULL` |

---

## 현재 제한사항 / 후속 작업

- **캐시 정책**: feed v3(글로벌/토큰), detail v2, trending v2는 Redis cache-aside(기본 TTL 및
  late-fill 상한 60초)입니다. ranking은 generation cache(v2 키)를 사용하며 쓰기 시 generation을
  증가시킵니다. create/edit/delete 등 관련 쓰기는 feed/detail/trending을 무효화하고 ranking generation을
  갱신합니다. Redis 장애 시 라이브 쿼리로 폴백합니다.
- `GET /dev-post`, `GET /dev-post/ranking`의 `direction` 쿼리 파라미터는 파싱되지만 정렬에 반영되지
  않습니다(피드는 항상 최신순, 랭킹은 항상 좋아요 내림차순).
- 좋아요 취소(`DELETE .../like`)는 게시물 존재 여부를 검증하지 않아 좋아요(`POST`)와 404 동작이
  비대칭입니다 (위 [12번 항목](#12-좋아요-취소-delete-dev-postpost_idlike) 참고).

---

## Pin 기능 롤아웃 및 롤백

- API보다 먼저 additive migration의 migrations `v2` squash commit을 배포합니다.
- 배포된 API의 `DEVPOST_FEED_EXPIRATION <= 60000`을 확인합니다.
- 기존 API 노드가 모두 drain될 때까지 frontend pin action을 비활성 상태로 유지합니다.
- 마지막 기존 노드가 drain된 뒤 mixed v2 feed state가 만료되도록 60초를 기다립니다. Redis 전체 purge를 하지 않는다.
- frontend pin action을 활성화한 뒤 pin endpoint의 4xx/5xx 비율, primary/unique constraint error,
  feed cache hit/miss, read-replica lag 징후를 모니터링합니다.
- API 롤백이 필요하면 API binary만 롤백합니다. additive dev_post_pin 테이블과 mapping은 유지합니다.
  기존 코드에 무해하며 이후 호환 배포에서 그대로 재사용할 수 있습니다.
- 승인 후 API PR을 squash merge할 때 feature branch를 삭제하지 않습니다. 이어서
  `git fetch origin v2:v2`를 실행하고 local `v2`와 `origin/v2`가 같은 commit인지 확인합니다.

---

## TypeScript Interfaces

```typescript
// GET /dev-post
interface FeedQuery {
  token_id?: string;
  page?: number;   // default: 1, min: 1
  limit?: number;  // default: 10, min: 1, max: 100
}

// GET /dev-post/ranking
interface PaginationParams {
  page?: number;   // default: 1, min: 1
  limit?: number;  // default: 10, min: 1, max: 100
}

// POST /dev-post
interface CreatePollOptionRequest {
  label: string;
  image_uri?: string;
}
interface CreatePollRequest {
  options: CreatePollOptionRequest[];  // 2..=3
}
interface CreateDevPostRequest {
  token_id: string;
  title: string;            // required; non-blank, exact whitespace preserved
  body?: string;             // description only
  image_uris?: string[];   // 0..=4
  poll?: CreatePollRequest;
}

// PATCH /dev-post/{post_id}
interface EditDevPostRequest {
  title?: string | null;    // omitted=keep; null/blank=400; value=replaces
  body?: string;             // description only
  image_uris?: string[];   // full replace
}

// POST /dev-post/{post_id}/vote
interface VoteRequest {
  option_position: number;  // i16
}

// ---- 응답 ----
interface TokenSummary {
  token_id: string;
  name: string;
  symbol: string;
  image_uri: string | null;
  market_cap: string | null;  // USD 마켓캡. 랭킹=사람 단위("15875.71…"), 피드/상세/트렌딩=raw ×10^18 (후속 통일 예정). market 행 없으면 null, quote USD 가격 없으면 "0"
}
interface AuthorSummary {
  account_id: string;
  nickname: string | null;
  image_uri: string | null;
}
interface PollOptionResponse {
  position: number;   // 1..=3
  label: string;
  image_uri: string | null;
  vote_count: number;
}
interface PollResponse {
  closes_at: string;         // ISO8601, created_at + 14d
  is_closed: boolean;        // 읽기 시점 계산
  total_votes: number;
  options: PollOptionResponse[];
  my_vote_option: number | null;
}
interface DevPostResponse {
  id: string;                 // BIGINT as string
  token: TokenSummary;
  author: AuthorSummary;
  title: string;
  body: string;              // description only
  tweet_url: string | null;
  images: string[];
  poll: PollResponse | null;
  like_count: number;
  liked_by_me: boolean;
  is_edited: boolean;
  created_at: string;
  updated_at: string;
}

// GET /dev-post
interface DevPostListResponse {
  pin: DevPostResponse | null;
  posts: DevPostResponse[];
  total_count: number;
}
// GET /dev-post/trending
interface TrendingResponse {
  posts: DevPostResponse[];   // <= 3
}
// GET /dev-post/ranking
interface RankingRow {
  rank: number;
  token: TokenSummary;
  total_likes: number;
  post_count: number;
  last_posted_at: string | null;
}
interface RankingResponse {
  rankings: RankingRow[];
  total_count: number;
}
// POST /dev-post/image
interface UploadImageResponse {
  image_uri: string;
}
// like/unlike
interface LikeResponse {
  like_count: number;
  liked_by_me: boolean;
}
// vote
interface VoteResponse {
  total_votes: number;
  options: PollOptionResponse[];
  my_vote_option: number | null;
}
```

---

## API 엔드포인트 요약

| Method | Path | 인증 | 설명 |
|--------|------|------|------|
| GET | `/dev-post` | optional-auth | 피드 (전체 또는 `token_id` 필터) |
| GET | `/dev-post/trending` | optional-auth | 최근 7일 좋아요 상위 3개 (부족분은 최신 게시물로 채움) |
| GET | `/dev-post/ranking` | X | 코인별 누적 좋아요 랭킹 |
| GET | `/dev-post/{post_id}` | optional-auth | 게시물 상세 |
| POST | `/dev-post/image` | O | 이미지 업로드 (raw body → R2) |
| POST | `/dev-post` | O (creator only) | 게시물 생성 |
| PATCH | `/dev-post/{post_id}` | O (author only) | 게시물 수정 (본문/이미지) |
| DELETE | `/dev-post/{post_id}` | O (author only) | 소프트 삭제 |
| PUT | `/dev-post/{post_id}/pin` | O (current creator + current-creator-authored live post) | 최초/반복/교체 pin, 빈 204 |
| DELETE | `/dev-post/{post_id}/pin` | O (current creator only) | exact-post/반복 pin 해제, 빈 204 |
| POST | `/dev-post/{post_id}/like` | O | 좋아요 |
| DELETE | `/dev-post/{post_id}/like` | O | 좋아요 취소 |
| POST | `/dev-post/{post_id}/vote` | O | 투표 (변경 가능, 마감 전까지) |

### 제목 계약 (breaking)

생성 요청은 `{"token_id":"0x...","title":"Exact title\nwith newline","body":"Description only"}` 형태다. `title` 누락/null/blank는 400이며, 공백과 줄바꿈은 정확히 보존되고 title 전용 길이 제한은 없다. 전체 요청이 100000 bytes를 넘으면 413이다. 수정에서 title 생략은 기존 제목을 보존하고, null/blank는 400이다. 응답의 모든 표면(feed/detail/trending/ranking)은 title과 body를 별도 필드로 반환한다. body는 description only로 파싱한다.

구버전 reader의 오표시는 허용되지만 구버전 create의 400과 legacy body-only edit의 모호성은 호환 대상이 아니다. capability header, versioned endpoint, fallback/협상은 제공하지 않는다. OpenAPI에는 Dev Post 13개 operation과 CMS moderation 2개 operation이 등록된다.
