# CLAUDE.md — api-server

## 핵심 규칙

- EVM 주소는 EIP-55 체크섬이 canonical (`LOWER()` 비교 금지).
- **세션 핸드오프**: 세션 마무리 시 루트 `next_session.md`에 진행상황/다음 할 일을 기록한다. 새 세션은 **먼저 `next_session.md`를 읽고** 이어간다.
- **에이전트 모델 배정 (Fable-GPT, 2026-07-16 개정)**: 오케스트레이터=**Fable 5**(계획·레포 이해·아키텍처 결정·작업 분해·최종 리뷰), 실행자=**Codex GPT-5.6**(`/codex:rescue`로 위임 — 무거운 구현·디버깅·테스트 수정·리팩토링·멀티파일 편집). 기본 모델 Sol medium, 고난도 추론 Sol extra high, 계획 확정 후 순수 실행 Terra/Luna. Codex 작업은 목표를 좁고 구체적으로; 출력은 수용 전 오케스트레이터가 직접 검사(맹신 금지).
