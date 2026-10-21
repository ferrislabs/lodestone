use std::{
    collections::HashMap,
    pin::pin,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use tokio::sync::Notify;
use tracing::{Instrument, debug, info_span};

use crate::domain::llm::{
    LlmError, LlmProvider, LlmRequest, LlmResponse,
    provider::{MaxConcurrency, ProviderId},
};

pub(crate) fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateLoad {
    pub in_flight: u32,
    pub limit: u8,
}

#[derive(Debug)]
pub struct Gate {
    provider: ProviderId,
    load: Mutex<GateLoad>,
    released: Notify,
}

impl Gate {
    fn new(provider: ProviderId, limit: MaxConcurrency) -> Self {
        Self {
            provider,
            load: Mutex::new(GateLoad {
                in_flight: 0,
                limit: limit.get(),
            }),
            released: Notify::new(),
        }
    }

    pub fn load(&self) -> GateLoad {
        *locked(&self.load)
    }

    fn resize(&self, limit: MaxConcurrency) {
        locked(&self.load).limit = limit.get();
        self.released.notify_waiters();
    }

    fn try_enter(&self) -> Option<GateLoad> {
        let mut load = locked(&self.load);
        if load.in_flight < u32::from(load.limit) {
            load.in_flight += 1;
            Some(*load)
        } else {
            None
        }
    }

    pub async fn acquire(&self) -> Permit<'_> {
        loop {
            let mut released = pin!(self.released.notified());
            released.as_mut().enable();
            if let Some(load) = self.try_enter() {
                debug!(
                    counter.llm_calls_in_flight = 1i64,
                    provider = %self.provider,
                    in_flight = load.in_flight,
                    limit = load.limit,
                    "llm call admitted"
                );
                return Permit { gate: self };
            }
            released.await;
        }
    }
}

#[derive(Debug)]
pub struct Permit<'a> {
    gate: &'a Gate,
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let load = {
            let mut load = locked(&self.gate.load);
            load.in_flight = load.in_flight.saturating_sub(1);
            *load
        };
        self.gate.released.notify_waiters();
        debug!(
            counter.llm_calls_in_flight = -1i64,
            provider = %self.gate.provider,
            in_flight = load.in_flight,
            limit = load.limit,
            "llm call released"
        );
    }
}

#[derive(Debug, Default)]
pub struct ProviderGates {
    gates: Mutex<HashMap<ProviderId, Arc<Gate>>>,
}

impl ProviderGates {
    pub fn gate(&self, provider: ProviderId, limit: MaxConcurrency) -> Arc<Gate> {
        let mut gates = locked(&self.gates);
        let gate = gates
            .entry(provider)
            .or_insert_with(|| Arc::new(Gate::new(provider, limit)));
        if gate.load().limit != limit.get() {
            gate.resize(limit);
        }
        Arc::clone(gate)
    }

    pub fn resize(&self, provider: ProviderId, limit: MaxConcurrency) {
        if let Some(gate) = locked(&self.gates).get(&provider) {
            gate.resize(limit);
        }
    }

    pub fn forget(&self, provider: ProviderId) {
        locked(&self.gates).remove(&provider);
    }

    pub fn in_flight(&self, provider: ProviderId) -> u32 {
        locked(&self.gates)
            .get(&provider)
            .map_or(0, |gate| gate.load().in_flight)
    }
}

#[derive(Debug)]
pub struct Throttled<P> {
    provider: P,
    gate: Arc<Gate>,
}

impl<P> Throttled<P> {
    pub fn new(provider: P, gate: Arc<Gate>) -> Self {
        Self { provider, gate }
    }

    pub fn gate(&self) -> &Gate {
        &self.gate
    }

    pub fn inner(&self) -> &P {
        &self.provider
    }
}

impl<P: LlmProvider> LlmProvider for Throttled<P> {
    async fn complete(&self, request: &LlmRequest<'_>) -> Result<LlmResponse, LlmError> {
        let _permit = self.gate.acquire().await;
        let load = self.gate.load();
        let span = info_span!(
            "llm_call",
            provider = %self.gate.provider,
            in_flight = load.in_flight,
            limit = load.limit,
        );
        self.provider.complete(request).instrument(span).await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use tokio::{
        task::JoinSet,
        time::{sleep, timeout},
    };
    use uuid::Uuid;

    use super::*;
    use crate::domain::llm::TokenUsage;

    struct Slow {
        running: Mutex<(u32, u32)>,
    }

    impl Slow {
        fn new() -> Self {
            Self {
                running: Mutex::new((0, 0)),
            }
        }

        fn peak(&self) -> u32 {
            self.running.lock().unwrap().1
        }
    }

    impl LlmProvider for Slow {
        async fn complete(&self, _: &LlmRequest<'_>) -> Result<LlmResponse, LlmError> {
            {
                let mut running = self.running.lock().unwrap();
                running.0 += 1;
                running.1 = running.1.max(running.0);
            }
            sleep(Duration::from_millis(100)).await;
            self.running.lock().unwrap().0 -= 1;
            Ok(LlmResponse {
                output: json!({}),
                model: "slow".to_owned(),
                usage: TokenUsage::default(),
            })
        }
    }

    fn limit(value: u8) -> MaxConcurrency {
        MaxConcurrency::new(value).unwrap()
    }

    async fn call<P: LlmProvider>(provider: &P) {
        let schema = json!({});
        let request = LlmRequest {
            system: "system",
            instruction: "instruction",
            documents: &[],
            schema_name: "output",
            schema: &schema,
            max_tokens: 1,
            temperature: None,
        };
        provider.complete(&request).await.unwrap();
    }

    async fn run(calls: usize, gate: Arc<Gate>) -> Arc<Throttled<Slow>> {
        let provider = Arc::new(Throttled::new(Slow::new(), gate));
        let mut tasks = JoinSet::new();
        for _ in 0..calls {
            let provider = Arc::clone(&provider);
            tasks.spawn(async move { call(provider.as_ref()).await });
        }
        while tasks.join_next().await.is_some() {}
        provider
    }

    #[tokio::test(start_paused = true)]
    async fn calls_overlap_up_to_the_provider_limit_and_never_beyond() {
        let gates = ProviderGates::default();
        let provider = ProviderId(Uuid::now_v7());
        let throttled = run(10, gates.gate(provider, limit(3))).await;
        assert_eq!(throttled.provider.peak(), 3);
        assert_eq!(gates.in_flight(provider), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn the_gate_is_shared_per_provider_and_follows_a_new_limit() {
        let gates = ProviderGates::default();
        let provider = ProviderId(Uuid::now_v7());
        let first = gates.gate(provider, limit(2));
        let second = gates.gate(provider, limit(2));
        assert!(Arc::ptr_eq(&first, &second));
        gates.resize(provider, limit(5));
        assert_eq!(first.load().limit, 5);
        let throttled = run(12, Arc::clone(&first)).await;
        assert_eq!(throttled.provider.peak(), 5);
        let other = gates.gate(ProviderId(Uuid::now_v7()), limit(1));
        assert!(!Arc::ptr_eq(&first, &other));
    }

    #[tokio::test(start_paused = true)]
    async fn in_flight_calls_are_visible_while_they_run() {
        let gates = Arc::new(ProviderGates::default());
        let provider = ProviderId(Uuid::now_v7());
        let throttled = Arc::new(Throttled::new(Slow::new(), gates.gate(provider, limit(2))));
        let mut tasks = JoinSet::new();
        for _ in 0..3 {
            let throttled = Arc::clone(&throttled);
            tasks.spawn(async move { call(throttled.as_ref()).await });
        }
        sleep(Duration::from_millis(50)).await;
        assert_eq!(gates.in_flight(provider), 2);
        while tasks.join_next().await.is_some() {}
        assert_eq!(gates.in_flight(provider), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn a_dropped_call_gives_its_permit_back() {
        let gates = ProviderGates::default();
        let provider = ProviderId(Uuid::now_v7());
        let gate = gates.gate(provider, limit(1));
        let throttled = Throttled::new(Slow::new(), Arc::clone(&gate));
        let dropped = timeout(Duration::from_millis(10), call(&throttled)).await;
        assert!(dropped.is_err());
        assert_eq!(gate.load().in_flight, 0);
        call(&throttled).await;
    }
}
