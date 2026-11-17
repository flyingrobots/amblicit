use amblicit_syntax::ProviderResponseLimits;
use wasmtime::ResourceLimiter;

/// Complete explicit containment policy for one provider invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderHostLimits {
    /// Maximum deterministic WIT logical bytes in the request.
    pub max_input_bytes: u64,
    /// Maximum logical response bytes the request may authorize.
    pub max_output_bytes: u64,
    /// Maximum provider-authored diagnostic bytes after lifting.
    pub max_diagnostic_bytes: u64,
    /// Maximum guest linear-memory allocation in one fresh store.
    pub max_wasm_memory_bytes: usize,
    /// Maximum elements in any guest table.
    pub max_table_elements: usize,
    /// Maximum component/core instances in one store.
    pub max_instances: usize,
    /// Maximum guest linear memories in one store.
    pub max_memories: usize,
    /// Maximum guest tables in one store.
    pub max_tables: usize,
    /// Deterministic Wasm instruction/work fuel.
    pub max_wasm_fuel: u64,
    /// Wasmtime guest-to-host lifting/allocation fuel for one call.
    pub max_hostcall_bytes: usize,
    /// Maximum bounded diagnostic bytes retained from the engine.
    pub max_host_diagnostic_bytes: usize,
}

/// Maximum WIT logical request bytes the application build sends to one
/// provider invocation.
const APPLICATION_BUILD_MAX_INPUT_BYTES: u64 = 1024 * 1024;

/// Wasm fuel the application build grants per admitted input byte.
///
/// Provider work grows with the request, so the fuel ceiling is sized from
/// the input cap instead of from today's programs. Measured on Echo's
/// `echo.dpo@2` lowerer and verifier (issue #326), the highest rate was about
/// 2,550 fuel per input byte: the lowerer on a 546,592-byte request, the
/// largest that fits its 16 MiB memory limit, used 1,393,523,207 fuel. The
/// verifier's highest rate was about 1,440 per byte. 4,096 leaves at least
/// 1.6 times headroom over the measured rate at every admitted input size.
pub const WASM_FUEL_PER_INPUT_BYTE: u64 = 4096;

impl ProviderHostLimits {
    /// The containment policy the application build applies to every
    /// lowerer and verifier invocation.
    ///
    /// `max_wasm_fuel` is `max_input_bytes * WASM_FUEL_PER_INPUT_BYTE`,
    /// 4,294,967,296. It is one fixed constant because fuel exhaustion is
    /// part of the build verdict: the same request must succeed or fail with
    /// `FuelExhausted` on every host.
    pub const APPLICATION_BUILD: Self = Self {
        max_input_bytes: APPLICATION_BUILD_MAX_INPUT_BYTES,
        max_output_bytes: 3 * 1024 * 1024,
        max_diagnostic_bytes: 3 * 1024 * 1024,
        max_wasm_memory_bytes: 16 * 1024 * 1024,
        max_table_elements: 10_000,
        max_instances: 100,
        max_memories: 8,
        max_tables: 8,
        max_wasm_fuel: APPLICATION_BUILD_MAX_INPUT_BYTES * WASM_FUEL_PER_INPUT_BYTE,
        max_hostcall_bytes: 4 * 1024 * 1024,
        max_host_diagnostic_bytes: 512,
    };
}

/// The response limits the application build places in every lowerer and
/// verifier request.
pub const APPLICATION_BUILD_RESPONSE_LIMITS: ProviderResponseLimits = ProviderResponseLimits {
    max_output_count: 1,
    max_diagnostic_count: 8,
    max_total_response_bytes: 64 * 1024,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeniedResource {
    Memory,
    Table,
}

#[derive(Debug)]
pub(crate) struct InvocationLimiter {
    limits: ProviderHostLimits,
    denied: Option<DeniedResource>,
}

impl InvocationLimiter {
    pub(crate) const fn new(limits: ProviderHostLimits) -> Self {
        Self {
            limits,
            denied: None,
        }
    }

    pub(crate) const fn denied(&self) -> Option<DeniedResource> {
        self.denied
    }
}

impl ResourceLimiter for InvocationLimiter {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > self.limits.max_wasm_memory_bytes {
            self.denied = Some(DeniedResource::Memory);
            Ok(false)
        } else {
            Ok(true)
        }
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > self.limits.max_table_elements {
            self.denied = Some(DeniedResource::Table);
            Ok(false)
        } else {
            Ok(true)
        }
    }

    fn instances(&self) -> usize {
        self.limits.max_instances
    }

    fn memories(&self) -> usize {
        self.limits.max_memories
    }

    fn tables(&self) -> usize {
        self.limits.max_tables
    }
}
