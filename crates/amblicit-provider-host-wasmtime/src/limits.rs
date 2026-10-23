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
