# Gift Bot — X Webhook 통합 (폐기 / Post-mortem)

> **상태: 폐기됨 (2026-05-30).** api-server에 이식했던 gift-bot webhook 수신부 +
> 온체인 consumer + reply를 **전부 제거**했다. 사유는 아래 "왜 안 되는가" 참조.
> 코드는 git 히스토리(PR #102~#108)에 남아있고, consumer/parser/`gift_tweet`
> 멱등 INSERT 로직은 후속 recent-search 방식에서 복구·재사용 가능하다.

## 무엇을 시도했나

기존 `gift-bot` 워크스페이스(producer + consumer + reply)를 api-server로 이전하면서,
X 인입을 **filtered stream → Account Activity API(AAA) webhook**으로 교체했다.
나머지 로직(parser / preflight / `setReceiver` / reconcile / reply / `gift_tweet`
상태머신)은 gift-bot 코드를 1:1로 이식했다.

- 설계: [`../superpowers/specs/2026-05-29-gift-bot-webhook-migration-design.md`](../superpowers/specs/2026-05-29-gift-bot-webhook-migration-design.md)
- 플랜: [`../superpowers/plans/2026-05-29-gift-webhook-migration.md`](../superpowers/plans/2026-05-29-gift-webhook-migration.md)

## 왜 안 되는가 (폐기 사유)

**AAA webhook은 "구독 유저가 알림(notification)을 받을 멘션"만 전달한다.** gift 활성화는
임의 계정이 `@nadfunnews`를 멘션해서 일어나는데, 그 멘션이 정확히 X가 거르는 대상이다.

1. **v2 Webhooks는 전송 트랜스포트일 뿐.** 실제 이벤트는 그 위에 붙는 제품(XAA / **AAA** /
   Filtered Stream) 중 하나의 *subscription*이 보낸다. 우리가 쓴 건 AAA.
2. **AAA는 알림 품질필터에 종속.** X 공식 FAQ: *"Account Activity API only delivers events
   when the subscription user would receive a notification from X and could see the event
   publicly."* 구독 유저(`@nadfunnews`)가 **안 팔로우한 / 신생 / 저품질 계정**의 멘션은
   알림이 안 떠서 → `tweet_create_event`가 전송되지 않는다.
3. **계정 설정으로도 못 막는다.** 알림 품질필터/고급필터를 다 꺼도 X 플랫폼 기본 스팸 억제는
   사용자가 끌 수 없어 누락이 남는다.
4. **filtered stream과 동작 비보존.** 원본 gift-bot은 filtered stream으로 룰 매칭 트윗을
   **전수신**했다. stream → AAA webhook 전환은 "transport만 바꾸고 로직 동일"이 이 지점에서
   깨진다.

## 검증 (2026-05-30)

webhook 인프라는 **전부 정상**임을 확인한 뒤에도 멘션이 안 들어왔다. 단계별 실측:

| 확인 항목 | 결과 |
|---|---|
| `/x/webhook` 라우트 + CRC | `valid:true` (X CRC GET 통과, 새 빌드 배포 확인) |
| 구독 계정 | `subscriptions.all/list` = `@nadfunnews` user_id 정확 일치 |
| localhost POST → 핸들러 | `gift webhook POST received` 로그 + 401 (핸들러 정상) |
| 공개 URL POST → 핸들러 | 동일 (cloudflare → haproxy → origin 경로 정상) |
| Cloudflare 방화벽 이벤트 | `/x/webhook` 차단 **0건** |
| **Cloudflare 트래픽 로그** | X(ASN 13414)發 **GET CRC만 존재, POST 0건** |

마지막 줄이 결정타: CF 엣지는 X가 보낸 모든 요청을 기록하는데 `/x/webhook`로 온 **POST가
0개**였다 = **X가 멘션 이벤트를 애초에 안 보냈다.** 인프라/CRC/서명/라우트/구독/단일리더
consumer는 전부 정상이었고, 원인은 100% X-side(품질필터)로 확정됐다.

## 대안 (후속)

오픈 gift 캠페인의 신뢰 가능한 수신 수단은 **X API v2 recent search 폴링**이다.

- 리더에서만 주기 폴링: `GET /2/tweets/search/recent?query=@nadfunnews #Nadfun -is:retweet` + `since_id`
- 결과 → GiftParser → `gift_tweet` `ON CONFLICT DO NOTHING` → consumer가 처리
- 품질필터 무관하게 모든 멘션 캐치, 지속 연결 없음(stateless API에 적합), App Bearer면 OAuth1 서명 불필요
- consumer/parser/`gift_tweet` 로직은 git PR #102~#108에서 복구해 재사용

`gift_tweet` 테이블은 공유 `Naddotfun/migrations` 서브모듈(`0020_gift_tweet.sql`)에 있고
이번 제거에서 건드리지 않았다.
