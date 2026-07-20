# chore/giwa-monad-residue-cleanup

## Purpose

nads-pump(Monad) 계보에서 복제한 api-server를 GIWA 체인(ETH L2, wrapped-native = WETH `0x4200...0006`)용으로 정리한다. 값 수준 교체만 수행하고 리팩터링은 하지 않는다.

## Changes

- `config.rs`: 필수 env `WMON` → `WETH` (사용처 `hype/mod.rs`의 로컬명·에러 문구 포함)
- `services/pricing/defillama.rs`: `DEFILLAMA_CHAIN` 기본값 `monad` → `ethereum`, 문서/픽스처 갱신
- `services/auth/mod.rs`: chain ID 오류 문구에서 "Monad test chain" 제거 (로직은 기존대로 `CHAIN_ID` env 주도)
- `services/terminal/mod.rs`: coingecko id `monad` → `ethereum`
- 테스트 픽스처: `token/order.rs`, `dividend/mod.rs`의 quote/whitelist 시드 `Monad/MON` → `Wrapped Ether/WETH`
- 보존: raffle 레거시 wire 계약(`%_MONAD`, `total_monad`), `emonad` 테스트 픽스처명
- 검증: build + lib 319 passed / 6 failed — 실패 6건은 전부 dev_post 하니스 문제로 **베이스라인에서도 동일 실패**(무관). 실행 Codex(gpt-5.6-sol medium), 검증·커밋 orchestrator

## Outcome

- (머지 시 작성)
