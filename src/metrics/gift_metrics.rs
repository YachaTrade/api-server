use std::sync::atomic::{AtomicU64, Ordering};

/// Gift 웹훅/소비자/리플라이 메트릭
pub struct GiftMetrics {
    /// 서명 검증을 통과한 웹훅 POST 배치 수신 건수
    pub webhook_events: AtomicU64,
    /// 웹훅 서명 검증 실패 건수
    pub webhook_signature_failures: AtomicU64,
    /// CRC challenge 응답 건수
    pub webhook_crc: AtomicU64,
    /// 웹훅을 통해 DB에 insert 된 row 수
    pub webhook_ingested: AtomicU64,
    /// 소비자 setReceiver tx 성공 건수
    pub tx_success: AtomicU64,
    /// 소비자 setReceiver tx 실패 건수
    pub tx_failure: AtomicU64,
    /// 리플라이 전송 성공 건수
    pub reply_success: AtomicU64,
    /// 리플라이 전송 실패 건수
    pub reply_failure: AtomicU64,
    /// DB read 오류 건수
    pub db_read_errors: AtomicU64,
}

impl Default for GiftMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl GiftMetrics {
    pub fn new() -> Self {
        Self {
            webhook_events: AtomicU64::new(0),
            webhook_signature_failures: AtomicU64::new(0),
            webhook_crc: AtomicU64::new(0),
            webhook_ingested: AtomicU64::new(0),
            tx_success: AtomicU64::new(0),
            tx_failure: AtomicU64::new(0),
            reply_success: AtomicU64::new(0),
            reply_failure: AtomicU64::new(0),
            db_read_errors: AtomicU64::new(0),
        }
    }

    pub fn inc_webhook_event(&self) {
        self.webhook_events.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_signature_failure(&self) {
        self.webhook_signature_failures
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_crc(&self) {
        self.webhook_crc.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_ingested(&self, n: u64) {
        self.webhook_ingested.fetch_add(n, Ordering::Relaxed);
    }

    pub fn record_tx_success(&self) {
        self.tx_success.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_tx_failure(&self) {
        self.tx_failure.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_reply_success(&self) {
        self.reply_success.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_reply_failure(&self) {
        self.reply_failure.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_db_read_error(&self) {
        self.db_read_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// 모든 카운터 스냅샷을 반환:
    /// (webhook_events, webhook_signature_failures, webhook_crc,
    ///  webhook_ingested, tx_success, tx_failure,
    ///  reply_success, reply_failure, db_read_errors)
    #[allow(clippy::type_complexity)]
    pub fn get_values(&self) -> (u64, u64, u64, u64, u64, u64, u64, u64, u64) {
        (
            self.webhook_events.load(Ordering::Relaxed),
            self.webhook_signature_failures.load(Ordering::Relaxed),
            self.webhook_crc.load(Ordering::Relaxed),
            self.webhook_ingested.load(Ordering::Relaxed),
            self.tx_success.load(Ordering::Relaxed),
            self.tx_failure.load(Ordering::Relaxed),
            self.reply_success.load(Ordering::Relaxed),
            self.reply_failure.load(Ordering::Relaxed),
            self.db_read_errors.load(Ordering::Relaxed),
        )
    }
}
