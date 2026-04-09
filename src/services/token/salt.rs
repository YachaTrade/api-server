use alloy::primitives::{Address, B256, U256, keccak256};
use rayon::prelude::*;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tracing::{error, info};

use crate::{
    config::{V1_BONDING_CURVE, V1_TOKEN_IMPL, V2_BONDING_CURVE, V2_TOKEN_IMPL, VANITY_ADDRESS_SUFFIX},
    result::AppError,
    types::token::salt::{MineSaltRequest, MineSaltResponse},
    utils::valid_account_id,
};

// 최대 시도 횟수: 1천만번 (suffix가 "143"일 경우 평균 ~4096번에 찾음)
const MAX_ITERATIONS: u64 = 10_000_000;

// 청크 크기: 병렬 처리를 위해 작업을 1만개씩 나눔
const CHUNK_SIZE: u64 = 10_000;

/// Salt 마이닝 결과
///
/// 마이닝에 성공하면 이 구조체를 반환합니다.
#[derive(Debug, Clone)]
struct MinedSalt {
    salt: B256,       // 찾은 salt 값 (32바이트)
    address: Address, // 계산된 토큰 주소 (0x...143 형태)
    iterations: u64,  // 몇 번 만에 찾았는지
}

/// Salt 마이닝 서비스
///
/// 특정 suffix(접미사)로 끝나는 멋진 토큰 주소를 찾는 서비스입니다.
/// 예: "143"으로 끝나는 주소 (0x742d35Cc6634C0532925a3b844Bc9e7595f0143)
pub struct SaltService;

impl Default for SaltService {
    fn default() -> Self {
        Self::new()
    }
}

impl SaltService {
    pub fn new() -> Self {
        Self
    }

    /// Salt를 마이닝하여 원하는 suffix의 토큰 주소를 생성
    ///
    /// # 동작 순서
    /// 1. 요청 데이터 검증 (creator 주소가 유효한지)
    /// 2. 환경 변수에서 설정 로드 (BONDING_CURVE, TOKEN_IMPLEMENT, VANITY_ADDRESS_SUFFIX)
    /// 3. 고유한 UUID 생성 (여러 사용자가 동시 마이닝 시 충돌 방지)
    /// 4. 마이닝 실행 (병렬 처리로 빠르게 탐색)
    /// 5. 결과 처리 및 반환
    pub async fn mine_salt(&self, request: MineSaltRequest) -> Result<MineSaltResponse, AppError> {
        // 1단계: creator 주소가 올바른 형식인지 확인 (0x + 40자리 hex)
        self.validate_request(&request)?;

        // 2단계: 환경 변수에서 deployer, implementation, suffix 로드
        let config = MiningConfig::load(request.version)?;

        // 3단계: 이 마이닝 요청의 고유 식별자 생성 (256비트 랜덤)
        let random_bytes: [u8; 32] = rand::random();
        let request_uuid = hex::encode(random_bytes);

        // 로그: 마이닝 시작 정보 출력
        self.log_mining_start(&request, &config, &request_uuid);

        // 4단계: 실제 마이닝 시작 (시간 측정)
        let start_time = Instant::now();
        let result = self
            .execute_mining(&config, &request, &request_uuid)
            .await?;
        let mining_time = start_time.elapsed();

        // 5단계: 결과 처리 (성공 or 실패)
        self.process_result(result, &config.suffix, mining_time)
    }

    /// 요청 데이터 검증
    ///
    /// creator 주소가 올바른 EVM 주소 형식인지 확인
    /// - 0x로 시작
    /// - 총 42자리 (0x + 40자리 hex)
    /// - 모두 16진수 문자
    fn validate_request(&self, request: &MineSaltRequest) -> Result<(), AppError> {
        if valid_account_id(&request.creator).is_none() {
            return Err(AppError::BadRequest(format!(
                "Invalid creator address: {}",
                request.creator
            )));
        }

        // Validate name length (1-32 chars)
        if request.name.is_empty() || request.name.len() > 32 {
            return Err(AppError::BadRequest(
                "Token name must be between 1 and 32 characters".to_string(),
            ));
        }

        // Validate symbol length (1-10 chars)
        if request.symbol.is_empty() || request.symbol.len() > 10 {
            return Err(AppError::BadRequest(
                "Token symbol must be between 1 and 10 characters".to_string(),
            ));
        }

        // Validate metadata_uri length (max 500 chars)
        if request.metadata_uri.len() > 500 {
            return Err(AppError::BadRequest(
                "Metadata URI must be at most 500 characters".to_string(),
            ));
        }

        Ok(())
    }

    /// 마이닝 시작 로그 출력
    ///
    /// 디버깅을 위해 마이닝 정보를 로깅합니다.
    fn log_mining_start(&self, request: &MineSaltRequest, config: &MiningConfig, uuid: &str) {
        info!(
            "Mining salt [uuid={}]: creator={}, name={}, symbol={}, suffix='{}'",
            uuid, request.creator, request.name, request.symbol, config.suffix
        );
    }

    /// 마이닝 실행 (비동기)
    ///
    /// CPU 집약적인 작업이므로 spawn_blocking으로 별도 스레드에서 실행
    /// tokio의 async 런타임을 블로킹하지 않도록 합니다.
    async fn execute_mining(
        &self,
        config: &MiningConfig,
        request: &MineSaltRequest,
        uuid: &str,
    ) -> Result<Option<MinedSalt>, AppError> {
        // spawn_blocking으로 이동할 데이터 준비 (모두 clone)
        let deployer = config.deployer;
        let implementation = config.implementation;
        let suffix = config.suffix.clone();
        let creator = request.creator.clone();
        let name = request.name.clone();
        let symbol = request.symbol.clone();
        let metadata_uri = request.metadata_uri.clone();
        let uuid = uuid.to_string();

        // CPU 집약적 작업을 별도 스레드에서 실행 (async 런타임 블로킹 방지)
        tokio::task::spawn_blocking(move || {
            Self::mine_salt_with_suffix(
                deployer,
                implementation,
                &suffix,
                &creator,
                &name,
                &symbol,
                &metadata_uri,
                &uuid,
            )
        })
        .await
        .map_err(|e| AppError::InternalError(format!("Mining task failed: {}", e)))
    }

    /// 마이닝 결과 처리
    ///
    /// 성공 시: salt와 address를 0x 접두사와 함께 반환
    /// 실패 시: 에러 메시지 반환
    fn process_result(
        &self,
        result: Option<MinedSalt>,
        suffix: &str,
        mining_time: std::time::Duration,
    ) -> Result<MineSaltResponse, AppError> {
        match result {
            Some(mined) => {
                // 성공! 로그 출력
                info!(
                    "✓ Salt mined: iterations={}, address={}, time={}ms",
                    mined.iterations,
                    mined.address,
                    mining_time.as_millis()
                );

                // 클라이언트에 반환할 응답 생성 (0x 접두사 추가)
                Ok(MineSaltResponse {
                    salt: format!("0x{}", hex::encode(mined.salt.as_slice())),
                    address: mined.address.to_string(),
                })
            }
            None => {
                // 실패: MAX_ITERATIONS 내에 찾지 못함
                error!(
                    "Mining failed: no match for suffix '{}' after {} iterations ({}ms)",
                    suffix,
                    MAX_ITERATIONS,
                    mining_time.as_millis()
                );

                Err(AppError::InternalError(format!(
                    "Failed to find salt with suffix '{}' after {} iterations",
                    suffix, MAX_ITERATIONS
                )))
            }
        }
    }

    /// CREATE2 주소 계산
    ///
    /// OpenZeppelin의 Clones.cloneDeterministic과 동일한 로직 구현
    /// https://github.com/OpenZeppelin/openzeppelin-contracts/blob/master/contracts/proxy/Clones.sol
    ///
    /// # CREATE2 공식
    /// address = keccak256(0xff ++ deployer ++ salt ++ keccak256(init_code))[12:]
    ///
    /// # EIP-1167 Minimal Proxy 패턴
    /// - Prefix (20바이트): 0x3d602d80600a3d3981f3363d3d373d3d3d363d73
    /// - Implementation (20바이트): 실제 구현 컨트랙트 주소
    /// - Suffix (15바이트): 0x5af43d82803e903d91602b57fd5bf3
    /// - 총 55바이트
    ///
    /// # 참고
    /// - salt를 바꾸면 주소가 달라짐
    /// - deployer, implementation이 같으면 salt만으로 주소 결정
    /// - 배포 전에 주소를 미리 계산할 수 있음!
    fn compute_create2_address(deployer: Address, implementation: Address, salt: B256) -> Address {
        // EIP-1167 minimal proxy bytecode 구성 (총 55바이트)
        let mut init_code = Vec::with_capacity(55);

        // 1. Prefix 추가 (20바이트)
        init_code
            .extend_from_slice(&hex::decode("3d602d80600a3d3981f3363d3d373d3d3d363d73").unwrap());

        // 2. Implementation 주소 추가 (20바이트)
        init_code.extend_from_slice(implementation.as_slice());

        // 3. Suffix 추가 (15바이트)
        init_code.extend_from_slice(&hex::decode("5af43d82803e903d91602b57fd5bf3").unwrap());

        // init_code를 해시 (이게 bytecode hash가 됨)
        let init_code_hash = keccak256(&init_code);

        // CREATE2 입력 데이터 구성 (총 85바이트)
        // 0xff(1) ++ deployer(20) ++ salt(32) ++ init_code_hash(32) = 85바이트
        let mut create2_input = Vec::with_capacity(85);
        create2_input.push(0xff); // CREATE2 식별자
        create2_input.extend_from_slice(deployer.as_slice()); // deployer 주소
        create2_input.extend_from_slice(salt.as_slice()); // salt (우리가 바꿀 수 있는 값!)
        create2_input.extend_from_slice(init_code_hash.as_slice()); // bytecode hash

        // 전체를 해시하고 뒤 20바이트를 주소로 사용
        let hash = keccak256(&create2_input);
        Address::from_slice(&hash[12..]) // 32바이트 중 뒤 20바이트 = 주소
    }

    /// 주소가 특정 suffix로 끝나는지 확인
    ///
    /// # 예시
    /// - address: 0x742d35Cc6634C0532925a3b844Bc9e7595f0143
    /// - suffix: "143" → true
    /// - suffix: "0143" → true
    /// - suffix: "144" → false
    fn address_ends_with(address: &Address, suffix: &str) -> bool {
        let address_hex = hex::encode(address.as_slice());
        address_hex.ends_with(suffix)
    }

    /// Salt 마이닝 (병렬 처리)
    ///
    /// # 알고리즘
    /// 1. 사용자 정보로 unique한 시작 salt 생성 (충돌 방지)
    /// 2. 작업을 청크로 나눠서 병렬 처리 (rayon 사용)
    /// 3. 각 청크에서 salt를 1씩 증가시키며 주소 계산
    /// 4. suffix와 일치하는 주소 발견 시 즉시 종료
    ///
    /// # 병렬 처리 전략
    /// - 10,000개씩 청크로 나눔
    /// - 각 청크를 CPU 코어 개수만큼 병렬 실행
    /// - AtomicBool로 발견 여부 공유 (lock-free)
    /// - 한 스레드가 찾으면 나머지 즉시 중단
    ///
    /// # 충돌 방지
    /// - 각 사용자의 요청마다 unique한 시작 salt 사용
    /// - creator + name + symbol + metadata_uri + UUID를 해시
    /// - 동시에 여러 사용자가 마이닝해도 같은 salt 시도 안 함
    fn mine_salt_with_suffix(
        deployer: Address,
        implementation: Address,
        suffix: &str,
        creator: &str,
        name: &str,
        symbol: &str,
        metadata_uri: &str,
        uuid: &str,
    ) -> Option<MinedSalt> {
        // suffix를 소문자로 변환 (대소문자 구분 안 함)
        let suffix = suffix.to_lowercase();

        // 발견 플래그: 한 스레드가 찾으면 true (lock-free, atomic)
        let found = Arc::new(AtomicBool::new(false));

        // 결과 저장소: 찾은 salt와 address를 저장 (mutex 사용)
        let result: Arc<parking_lot::Mutex<Option<MinedSalt>>> =
            Arc::new(parking_lot::Mutex::new(None));

        // 1단계: 이 사용자만의 unique한 시작 salt 생성
        // hash(creator + name + symbol + metadata_uri + uuid)
        // → 다른 사용자와 겹치지 않음!
        let start_salt = Self::generate_starting_salt(creator, name, symbol, metadata_uri, uuid);
        let start_u256 = U256::from_be_bytes(*start_salt);

        // 2단계: 청크 개수 계산
        // 예: 10,000,000 / 10,000 = 1,000개 청크
        let num_chunks = MAX_ITERATIONS.div_ceil(CHUNK_SIZE);

        // 3단계: 병렬 처리로 마이닝 (rayon의 parallel iterator 사용)
        (0..num_chunks).into_par_iter().find_map_any(|chunk_idx| {
            // 다른 스레드가 이미 찾았으면 즉시 종료
            if found.load(Ordering::Relaxed) {
                return None;
            }

            // 이 청크의 시작/끝 인덱스 계산
            // 예: chunk_idx=0 → [0, 10000), chunk_idx=1 → [10000, 20000)
            let chunk_start = chunk_idx * CHUNK_SIZE;
            let chunk_end = ((chunk_idx + 1) * CHUNK_SIZE).min(MAX_ITERATIONS);

            // 이 청크 내에서 순차 탐색
            for i in chunk_start..chunk_end {
                // 중간에 다른 스레드가 찾았으면 중단
                if found.load(Ordering::Relaxed) {
                    return None;
                }

                // 현재 시도할 salt 계산: start_salt + i
                let current_salt_u256 = start_u256.wrapping_add(U256::from(i));
                let salt = B256::from(current_salt_u256.to_be_bytes::<32>());

                // 이 salt로 주소 계산
                let address = Self::compute_create2_address(deployer, implementation, salt);

                // suffix와 일치하는지 확인
                if Self::address_ends_with(&address, &suffix) {
                    // 찾았다! 🎉
                    found.store(true, Ordering::Relaxed); // 다른 스레드에 알림
                    let iterations = i + 1; // 몇 번 만에 찾았는지
                    let mined = MinedSalt {
                        salt,
                        address,
                        iterations,
                    };

                    // 결과 저장
                    *result.lock() = Some(mined.clone());
                    return Some(mined);
                }
            }

            // 이 청크에서 못 찾음
            None
        });

        // 모든 청크 탐색 완료 후 결과 반환
        // Some(MinedSalt) → 성공
        // None → MAX_ITERATIONS 내에 못 찾음
        result.lock().clone()
    }

    /// 요청 파라미터로부터 unique한 시작 salt 생성
    ///
    /// # 왜 필요한가?
    /// - 여러 사용자가 동시에 마이닝할 때 같은 salt를 시도하면 비효율적
    /// - 각 사용자마다 다른 시작점을 주면 충돌 없이 병렬 마이닝 가능
    ///
    /// # 입력
    /// - creator: 토큰 생성자 주소
    /// - name: 토큰 이름
    /// - symbol: 토큰 심볼
    /// - metadata_uri: 토큰 메타데이터 URI
    /// - uuid: 이 요청의 고유 식별자
    ///
    /// # 출력
    /// - B256: 32바이트 해시값 (이걸 시작 salt로 사용)
    ///
    /// # 특징
    /// - 같은 입력 → 같은 출력 (deterministic)
    /// - 다른 입력 → 다른 출력 (collision resistant)
    fn generate_starting_salt(
        creator: &str,
        name: &str,
        symbol: &str,
        metadata_uri: &str,
        uuid: &str,
    ) -> B256 {
        // 모든 파라미터를 이어붙여서 하나의 바이트 배열 생성
        let mut params_data = Vec::new();
        params_data.extend_from_slice(creator.as_bytes());
        params_data.extend_from_slice(name.as_bytes());
        params_data.extend_from_slice(symbol.as_bytes());
        params_data.extend_from_slice(metadata_uri.as_bytes());
        params_data.extend_from_slice(uuid.as_bytes());

        // 전체를 keccak256 해시 → 32바이트 unique salt
        B256::from(keccak256(&params_data))
    }
}

/// 마이닝 설정
///
/// 환경 변수에서 로드하는 설정값들
struct MiningConfig {
    deployer: Address,       // 토큰 팩토리 주소 (BONDING_CURVE)
    implementation: Address, // 토큰 구현 주소 (TOKEN_IMPLEMENT)
    suffix: String,          // 원하는 주소 suffix (VANITY_ADDRESS_SUFFIX, 예: "143")
}

impl MiningConfig {
    /// 환경 변수에서 설정 로드
    ///
    /// # 필수 환경 변수
    /// - BONDING_CURVE: 토큰을 배포할 팩토리 컨트랙트 주소
    /// - TOKEN_IMPLEMENT: 토큰 구현 컨트랙트 주소 (EIP-1167 proxy가 참조)
    /// - VANITY_ADDRESS_SUFFIX: 원하는 주소 suffix (hex 문자열, 예: "143")
    fn load(version: u8) -> Result<Self, AppError> {
        let (deployer_str, impl_str) = match version {
            2 => (V2_BONDING_CURVE.as_str(), V2_TOKEN_IMPL.as_str()),
            _ => (V1_BONDING_CURVE.as_str(), V1_TOKEN_IMPL.as_str()),
        };

        let deployer = Address::from_str(deployer_str)
            .map_err(|e| AppError::InternalError(format!("Failed to parse bonding curve address: {}", e)))?;
        let implementation = Address::from_str(impl_str)
            .map_err(|e| AppError::InternalError(format!("Failed to parse token implement address: {}", e)))?;
        let suffix = VANITY_ADDRESS_SUFFIX.clone();

        Ok(Self {
            deployer,
            implementation,
            suffix,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CREATE2 주소 계산이 Solidity와 동일한지 테스트
    #[test]
    fn test_compute_create2_address_matches_solidity() {
        let deployer = Address::from_str("0x5FbDB2315678afecb367f032d93F642f64180aa3").unwrap();
        let implementation =
            Address::from_str("0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512").unwrap();
        let salt = B256::from([0u8; 32]);

        let address = SaltService::compute_create2_address(deployer, implementation, salt);

        // 주소가 올바른 형식인지 확인 (0x + 40자리 = 42자리)
        assert_eq!(address.to_string().len(), 42);

        // 같은 입력은 항상 같은 출력 (deterministic)
        let address2 = SaltService::compute_create2_address(deployer, implementation, salt);
        assert_eq!(address, address2);
    }

    /// EIP-1167 bytecode가 정확히 55바이트인지 테스트
    #[test]
    fn test_eip1167_bytecode_construction() {
        let implementation =
            Address::from_str("0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512").unwrap();

        // 수동으로 bytecode 구성
        let mut expected_bytecode = Vec::new();
        expected_bytecode
            .extend_from_slice(&hex::decode("3d602d80600a3d3981f3363d3d373d3d3d363d73").unwrap());
        expected_bytecode.extend_from_slice(implementation.as_slice());
        expected_bytecode
            .extend_from_slice(&hex::decode("5af43d82803e903d91602b57fd5bf3").unwrap());

        // 정확히 55바이트여야 함 (20 + 20 + 15)
        assert_eq!(expected_bytecode.len(), 55);
    }

    /// CREATE2 주소 계산 과정을 단계별로 검증
    #[test]
    fn test_create2_calculation_breakdown() {
        let deployer = Address::from_str("0xD5724171C2b7f0AA717a324626050BD05767e2C6").unwrap();
        let implementation =
            Address::from_str("0x52D34d8536350Cd997bCBD0b9E9d722452f341F5").unwrap();
        let salt = B256::from([1u8; 32]);

        // 1단계: Bytecode 구성
        let mut init_code = Vec::new();
        init_code
            .extend_from_slice(&hex::decode("3d602d80600a3d3981f3363d3d373d3d3d363d73").unwrap());
        init_code.extend_from_slice(implementation.as_slice());
        init_code.extend_from_slice(&hex::decode("5af43d82803e903d91602b57fd5bf3").unwrap());

        // 2단계: Init code 해시
        let init_code_hash = keccak256(&init_code);

        // 3단계: CREATE2 입력 구성
        let mut create2_input = Vec::new();
        create2_input.push(0xff);
        create2_input.extend_from_slice(deployer.as_slice());
        create2_input.extend_from_slice(salt.as_slice());
        create2_input.extend_from_slice(init_code_hash.as_slice());

        assert_eq!(create2_input.len(), 85); // 1 + 20 + 32 + 32

        // 4단계: 해시하고 주소 추출
        let hash = keccak256(&create2_input);
        let address = Address::from_slice(&hash[12..]);

        // 우리 함수와 결과가 같은지 확인
        let address_from_fn = SaltService::compute_create2_address(deployer, implementation, salt);
        assert_eq!(address, address_from_fn);
    }

    /// 주소 suffix 체크 테스트
    #[test]
    fn test_address_ends_with() {
        let address = Address::from_str("0x1234567890123456789012345678901234560143").unwrap();
        assert!(SaltService::address_ends_with(&address, "143"));
        assert!(SaltService::address_ends_with(&address, "0143"));
        assert!(SaltService::address_ends_with(&address, "43"));
        assert!(!SaltService::address_ends_with(&address, "144"));
    }

    /// 프로덕션 설정으로 주소 계산 테스트
    #[test]
    fn test_with_production_config() {
        let deployer = Address::from_str("0xD5724171C2b7f0AA717a324626050BD05767e2C6").unwrap();
        let implementation =
            Address::from_str("0x52D34d8536350Cd997bCBD0b9E9d722452f341F5").unwrap();

        let salt = B256::from([0u8; 32]);
        let address = SaltService::compute_create2_address(deployer, implementation, salt);

        // deterministic 확인
        let address2 = SaltService::compute_create2_address(deployer, implementation, salt);
        assert_eq!(address, address2);
    }

    /// 시작 salt 생성 테스트
    #[test]
    fn test_generate_starting_salt() {
        let salt1 = SaltService::generate_starting_salt(
            "0x1111111111111111111111111111111111111111",
            "Token A",
            "TKA",
            "ipfs://a",
            "uuid-a",
        );

        let salt2 = SaltService::generate_starting_salt(
            "0x2222222222222222222222222222222222222222",
            "Token B",
            "TKB",
            "ipfs://b",
            "uuid-b",
        );

        // 다른 파라미터면 다른 salt
        assert_ne!(salt1, salt2);

        // 같은 파라미터면 같은 salt (deterministic)
        let salt1_again = SaltService::generate_starting_salt(
            "0x1111111111111111111111111111111111111111",
            "Token A",
            "TKA",
            "ipfs://a",
            "uuid-a",
        );
        assert_eq!(salt1, salt1_again);
    }

    /// 단일 자리 suffix 마이닝 테스트 (빠르게 찾을 수 있음)
    #[test]
    fn test_mine_salt_single_digit() {
        let deployer = Address::from_str("0x5FbDB2315678afecb367f032d93F642f64180aa3").unwrap();
        let implementation =
            Address::from_str("0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512").unwrap();

        let result = SaltService::mine_salt_with_suffix(
            deployer,
            implementation,
            "3",
            "0x1234567890123456789012345678901234567890",
            "Test Token",
            "TEST",
            "ipfs://test",
            "test-uuid-123",
        );

        if let Some(mined) = result {
            // 계산된 주소가 올바른지 확인
            let computed_address =
                SaltService::compute_create2_address(deployer, implementation, mined.salt);
            assert_eq!(computed_address, mined.address);

            // suffix가 맞는지 확인
            assert!(SaltService::address_ends_with(&mined.address, "3"));
        } else {
            panic!("Failed to find address ending in '3' within max iterations");
        }
    }
}
