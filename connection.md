# RDS 연결 풀 설정 가이드

## 환경 사양
- **Primary DB (쓰기 전용)**: 글로벌 Writer RDS Proxy
- **Replica DB (읽기 전용)**: 싱가포르/테스트 지역 전용 r6gd.xlarge (4 vCPU, 32GB 메모리)
- **RDS Proxy**: 최대 사용률 80%로 제한
- **Fargate 오토스케일링**: 1-5 태스크 (각 4GB 메모리)

## 연결 제한
- **r6gd.xlarge max_connections**: 약 3,700개
- **RDS Proxy 80% 사용**: 3,700 × 0.8 = 2,960개 사용 가능

## 연결 풀 설정 값

### PRIMARY (쓰기 전용) 설정

#### 1. PG_PRIMARY_MAX_CONNECTIONS = 30
- **선택 이유**:
  - 쓰기 작업은 읽기보다 적음
  - 글로벌 Writer를 여러 지역이 공유
  - 쓰기 부하 분산 필요

#### 2. PG_PRIMARY_MIN_CONNECTIONS = 3
- **선택 이유**:
  - 최소한의 쓰기 연결 유지
  - 리소스 효율적 사용

#### 3. PG_PRIMARY_MAX_LIFETIME_SECS = 300 (5분)
- **선택 이유**:
  - 글로벌 연결의 안정성 확보
  - 주기적 연결 갱신

#### 4. PG_PRIMARY_ACQUIRE_TIMEOUT_SECS = 10
- **선택 이유**:
  - 쓰기 작업의 중요성 고려
  - 네트워크 지연 허용

#### 5. PG_PRIMARY_IDLE_TIMEOUT_SECS = 30
- **선택 이유**:
  - 빠른 연결 재활용
  - 글로벌 리소스 효율화

#### 6. PG_PRIMARY_STATEMENT_CACHE_CAPACITY = 200
- **선택 이유**:
  - 주요 쓰기 쿼리만 캐싱
  - 메모리 효율적 사용

### REPLICA (읽기 전용) 설정

#### 1. PG_REPLICA_MAX_CONNECTIONS = 350
- **계산 근거**:
  - 최대 Fargate 태스크: 5개
  - 태스크당 최대 연결: 350개
  - 총 최대 연결: 5 × 350 = 1,750개
  - RDS Proxy 사용률: 1,750 / 2,960 = 59.1%
- **선택 이유**:
  - API 트래픽의 대부분이 읽기 작업
  - 높은 동시성 지원
  - r6gd.xlarge의 여유 리소스 활용
  - RDS Proxy 한계 내에서 안전한 여유분 확보 (40.9%)

#### 2. PG_REPLICA_MIN_CONNECTIONS = 20
- **선택 이유**:
  - 빠른 읽기 응답 보장
  - 콜드 스타트 최소화
  - 항상 준비된 연결 풀
  - r6gd.xlarge의 충분한 메모리로 최소 연결 증가

#### 3. PG_REPLICA_MAX_LIFETIME_SECS = 300 (5분)
- **선택 이유**:
  - RDS Proxy와 조화
  - 안정적인 연결 관리

#### 4. PG_REPLICA_ACQUIRE_TIMEOUT_SECS = 5
- **선택 이유**:
  - 읽기 작업의 빠른 응답 필요
  - 사용자 경험 최적화

#### 5. PG_REPLICA_IDLE_TIMEOUT_SECS = 30
- **선택 이유**:
  - 효율적인 연결 재사용
  - RDS Proxy 최적화

#### 6. PG_REPLICA_STATEMENT_CACHE_CAPACITY = 1500
- **선택 이유**:
  - 다양한 읽기 쿼리 캐싱
  - 읽기 성능 최대화
  - r6gd.xlarge의 풍부한 메모리 활용 (32GB)
  - Fargate 4GB 메모리로 여유있는 캐시 설정

## 성능 분석

### 연결 풀 사용률
#### Primary (쓰기)
- **정상 시**: 3개 태스크 × 30 = 90개
- **피크 시**: 5개 태스크 × 30 = 150개
- **글로벌 Writer 고려**: 충분히 보수적

#### Replica (읽기)
- **정상 시**: 3개 태스크 × 350 = 1,050개 (35.5%)
- **피크 시**: 5개 태스크 × 350 = 1,750개 (59.1%)
- **여유분**: 1,210개 (40.9%)

### 메모리 사용량 (태스크당 4GB)
- **태스크당 메모리**: 4096MB
- **Primary 연결**: 30 × 3MB = 90MB
- **Replica 연결**: 350 × 3MB = 1,050MB
- **Statement 캐시**: ~150MB
- **애플리케이션**: ~1,500MB (예상)
- **총 사용**: ~2,790MB (68%)
- **여유분**: ~1,300MB (32%)

## 워크로드 특성

### 읽기/쓰기 비율
- **읽기**: 90-95% (대부분의 API 요청)
- **쓰기**: 5-10% (사용자 액션, 업데이트)

### 최적화 포인트
1. **Replica 우선**: 읽기 연결에 더 많은 리소스 할당
2. **Primary 보수적**: 글로벌 Writer 보호
3. **캐시 활용**: Replica에 큰 Statement 캐시

## 모니터링 권장사항

### CloudWatch 메트릭
1. **RDS Proxy (Replica)**:
   - `DatabaseConnections` > 1,750: 경고
   - `ClientConnections` 추세 모니터링
   - 연결 풀 대기 시간

2. **RDS Proxy (Primary)**:
   - 글로벌 연결 수 모니터링
   - 쓰기 지연 시간

### 알람 설정
- Replica 연결 사용률 > 80%
- Primary 연결 대기 > 2초
- 연결 획득 실패율 > 1%

## 튜닝 가이드

### Replica (읽기) 조정
1. **트래픽 증가 시**:
   - `PG_REPLICA_MAX_CONNECTIONS` 500까지 증가 가능
   - 단, 총 연결 수 2,960개 한계 고려 (5 태스크 × 500 = 2,500개 = 84.5%)

2. **응답 지연 시**:
   - `PG_REPLICA_MIN_CONNECTIONS` 증가
   - `PG_REPLICA_ACQUIRE_TIMEOUT_SECS` 감소

### Primary (쓰기) 조정
1. **쓰기 부하 증가**:
   - `PG_PRIMARY_MAX_CONNECTIONS` 신중히 증가
   - 글로벌 영향 고려

2. **연결 부족**:
   - 쓰기 패턴 분석 필요
   - 배치 처리 고려

## 결론
현재 설정은 읽기 중심의 API 워크로드에 최적화되어 있습니다. r6gd.xlarge (4 vCPU, 32GB 메모리)의 충분한 리소스를 활용하여 Replica DB에 350개 연결을 할당, 대부분의 트래픽을 효율적으로 처리합니다. Primary DB는 안정성을 우선으로 보수적으로 설정했습니다. RDS Proxy의 제한 내에서 안전하게 운영 가능하며 40.9%의 여유분을 확보한 구성입니다.

### 테스트/싱가포르 환경 설정 요약
```bash
# PRIMARY (변경 없음)
PG_PRIMARY_MAX_CONNECTIONS=30
PG_PRIMARY_MIN_CONNECTIONS=3
PG_PRIMARY_MAX_LIFETIME_SECS=300
PG_PRIMARY_ACQUIRE_TIMEOUT_SECS=10
PG_PRIMARY_IDLE_TIMEOUT_SECS=30
PG_PRIMARY_STATEMENT_CACHE_CAPACITY=200

# REPLICA (r6gd.xlarge 최적화)
PG_REPLICA_MAX_CONNECTIONS=350
PG_REPLICA_MIN_CONNECTIONS=20
PG_REPLICA_MAX_LIFETIME_SECS=300
PG_REPLICA_ACQUIRE_TIMEOUT_SECS=5
PG_REPLICA_IDLE_TIMEOUT_SECS=30
PG_REPLICA_STATEMENT_CACHE_CAPACITY=1500
```